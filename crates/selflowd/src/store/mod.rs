mod migrations;
mod types;

use std::path::Path;

use jiff::{Timestamp, civil};
use rusqlite::{Connection, OptionalExtension, Row, params};
use selflow_core::enforce::{DayLog, GrantedException, GrantedExtension, ShutdownDone};
use selflow_core::plan::RuleWindow;
use selflow_core::timing::Seed;
use selflow_core::versions::{ChangeKind, StoredChange, StoredVersion};
use selflow_platform::secure_fs::{private_dir, private_file};

use migrations::MIGRATIONS;
use types::ShutdownDetail;
pub use types::{Event, EventKind, StoreError};

const CHANGE_KINDS: [(ChangeKind, &str); 3] = [
    (ChangeKind::Tighten, "tighten"),
    (ChangeKind::Loosen, "loosen"),
    (ChangeKind::Neutral, "neutral"),
];

const EVENT_KINDS: [(EventKind, &str); 7] = [
    (EventKind::LockStarted, "lock_started"),
    (EventKind::LockEnded, "lock_ended"),
    (EventKind::ExtensionGranted, "extension_granted"),
    (EventKind::Shutdown, "shutdown"),
    (EventKind::ConfigSaved, "config_saved"),
    (EventKind::RuleShutdown, "rule_shutdown"),
    (EventKind::ExceptionGranted, "exception_granted"),
];

fn change_kind_text(kind: ChangeKind) -> &'static str {
    CHANGE_KINDS
        .iter()
        .find(|(candidate, _)| *candidate == kind)
        .map_or("neutral", |(_, text)| text)
}

fn change_kind_from(text: &str) -> Result<ChangeKind, StoreError> {
    CHANGE_KINDS
        .iter()
        .find(|(_, candidate)| *candidate == text)
        .map(|(kind, _)| *kind)
        .ok_or_else(|| StoreError::UnknownKind(text.to_owned()))
}

fn event_kind_text(kind: EventKind) -> &'static str {
    EVENT_KINDS
        .iter()
        .find(|(candidate, _)| *candidate == kind)
        .map_or("shutdown", |(_, text)| text)
}

fn event_kind_from(text: &str) -> Result<EventKind, StoreError> {
    EVENT_KINDS
        .iter()
        .find(|(_, candidate)| *candidate == text)
        .map(|(kind, _)| *kind)
        .ok_or_else(|| StoreError::UnknownKind(text.to_owned()))
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        if let Some(parent) = path.parent() {
            private_dir(parent)?;
        }
        private_file(path)?;
        Store::migrated(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Store, StoreError> {
        Store::migrated(Connection::open_in_memory()?)
    }

    fn migrated(conn: Connection) -> Result<Store, StoreError> {
        let applied: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        for (index, migration) in MIGRATIONS.iter().enumerate().skip(applied as usize) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(migration)?;
            tx.execute_batch(&format!("PRAGMA user_version = {}", index + 1))?;
            tx.commit()?;
        }
        Ok(Store { conn })
    }

    pub fn seed(&self) -> Result<Seed, StoreError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO meta (key, value) VALUES ('seed', ?1)",
            params![fastrand::u64(..).to_string()],
        )?;
        let text: String =
            self.conn
                .query_row("SELECT value FROM meta WHERE key = 'seed'", [], |row| {
                    row.get(0)
                })?;
        let value = text
            .parse()
            .map_err(|_| StoreError::UnknownKind(format!("seed {text}")))?;
        Ok(Seed(value))
    }

    pub fn save_version(
        &self,
        text: &str,
        saved_at: Timestamp,
        changes: &[StoredChange],
    ) -> Result<i64, StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO config_versions (saved_at, text) VALUES (?1, ?2)",
            params![timestamp_text(saved_at), text],
        )?;
        let id = tx.last_insert_rowid();
        for change in changes {
            tx.execute(
                "INSERT INTO config_changes (version_id, unit, kind, effective_at) VALUES (?1, ?2, ?3, ?4)",
                params![
                    id,
                    change.unit,
                    change_kind_text(change.kind),
                    timestamp_text(change.effective_at)
                ],
            )?;
        }
        tx.commit()?;
        Ok(id)
    }

    pub fn history_rows(&self) -> Result<(Vec<StoredVersion>, Vec<StoredChange>), StoreError> {
        Ok((self.versions()?, self.changes()?))
    }

    fn versions(&self) -> Result<Vec<StoredVersion>, StoreError> {
        let mut statement = self
            .conn
            .prepare("SELECT id, saved_at, text FROM config_versions ORDER BY id")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (id, saved_at, text) = row?;
            Ok(StoredVersion {
                id,
                saved_at: saved_at.parse()?,
                text,
            })
        })
        .collect()
    }

    fn changes(&self) -> Result<Vec<StoredChange>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT version_id, unit, kind, effective_at FROM config_changes ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (version_id, unit, kind, effective_at) = row?;
            Ok(StoredChange {
                version_id,
                unit,
                kind: change_kind_from(&kind)?,
                effective_at: effective_at.parse()?,
            })
        })
        .collect()
    }

    pub fn latest_text(&self) -> Result<Option<String>, StoreError> {
        self.text_of("SELECT text FROM config_versions ORDER BY id DESC LIMIT 1")
    }

    pub fn m1_text(&self) -> Result<Option<String>, StoreError> {
        self.text_of("SELECT text FROM config_versions_m1 ORDER BY id DESC LIMIT 1")
    }

    fn text_of(&self, query: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .conn
            .query_row(query, [], |row| row.get(0))
            .optional()?)
    }

    pub fn record_extension(&self, extension: &GrantedExtension) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO extensions (rule_set, rule_id, logical_date, granted_at, until, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                extension.rule_set,
                extension.rule_id,
                extension.logical_date.to_string(),
                timestamp_text(extension.granted_at),
                timestamp_text(extension.until),
                extension.reason
            ],
        )?;
        Ok(())
    }

    pub fn record_started(&self, window: &RuleWindow) -> Result<(), StoreError> {
        let key = window.key();
        let logical_date = window.logical_date.to_string();
        let stored: Option<String> = self
            .conn
            .query_row(
                "SELECT window FROM started_windows WHERE key = ?1 AND logical_date = ?2",
                params![key, logical_date],
                |row| row.get(0),
            )
            .optional()?;
        let Some(stored) = stored else {
            self.conn.execute(
                "INSERT INTO started_windows (key, logical_date, window) VALUES (?1, ?2, ?3)",
                params![key, logical_date, serde_json::to_string(window)?],
            )?;
            return Ok(());
        };
        let kept: RuleWindow = serde_json::from_str(&stored)?;
        if kept.starts_at.timestamp() <= window.starts_at.timestamp() {
            return Ok(());
        }
        let moved = RuleWindow {
            starts_at: window.starts_at.clone(),
            ..kept
        };
        self.conn.execute(
            "UPDATE started_windows SET window = ?3 WHERE key = ?1 AND logical_date = ?2",
            params![key, logical_date, serde_json::to_string(&moved)?],
        )?;
        Ok(())
    }

    pub fn night_log(&self, dates: &[civil::Date]) -> Result<DayLog, StoreError> {
        let mut log = DayLog::default();
        for date in dates {
            log.extensions.extend(self.extensions_on(*date)?);
            log.started.extend(self.started_on(*date)?);
            log.shutdowns.extend(self.shutdowns_on(*date)?);
            log.exceptions.extend(self.exceptions_on(*date)?);
        }
        Ok(log)
    }

    pub fn start_exception(
        &self,
        rule_id: &str,
        logical_date: civil::Date,
        at: Timestamp,
    ) -> Result<Timestamp, StoreError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO exceptions (rule_id, logical_date, requested_at) VALUES (?1, ?2, ?3)",
            params![rule_id, logical_date.to_string(), timestamp_text(at)],
        )?;
        Ok(self
            .exception_requested_at(rule_id, logical_date)?
            .unwrap_or(at))
    }

    pub fn restart_exception(
        &self,
        rule_id: &str,
        logical_date: civil::Date,
        at: Timestamp,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "UPDATE exceptions SET requested_at = ?3 WHERE rule_id = ?1 AND logical_date = ?2",
            params![rule_id, logical_date.to_string(), timestamp_text(at)],
        )?;
        Ok(())
    }

    pub fn exception_requested_at(
        &self,
        rule_id: &str,
        logical_date: civil::Date,
    ) -> Result<Option<Timestamp>, StoreError> {
        let requested_at: Option<String> = self
            .conn
            .query_row(
                "SELECT requested_at FROM exceptions WHERE rule_id = ?1 AND logical_date = ?2",
                params![rule_id, logical_date.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(requested_at.map(|text| text.parse()).transpose()?)
    }

    pub fn grant_exception(&self, exception: &GrantedException) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO exceptions (rule_id, logical_date, requested_at, granted_at, reason)
             VALUES (?1, ?2, ?3, ?3, ?4)
             ON CONFLICT (rule_id, logical_date) DO UPDATE
             SET granted_at = excluded.granted_at, reason = excluded.reason",
            params![
                exception.rule_id,
                exception.logical_date.to_string(),
                timestamp_text(exception.granted_at),
                exception.reason
            ],
        )?;
        Ok(())
    }

    fn exceptions_on(&self, date: civil::Date) -> Result<Vec<GrantedException>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT rule_id, granted_at, reason FROM exceptions
             WHERE logical_date = ?1 AND granted_at IS NOT NULL ORDER BY granted_at, rule_id",
        )?;
        let rows = statement.query_map(params![date.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (rule_id, granted_at, reason) = row?;
            Ok(GrantedException {
                rule_id,
                logical_date: date,
                granted_at: granted_at.parse()?,
                reason,
            })
        })
        .collect()
    }

    fn extensions_on(&self, date: civil::Date) -> Result<Vec<GrantedExtension>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT rule_set, rule_id, granted_at, until, reason FROM extensions
             WHERE logical_date = ?1 ORDER BY id",
        )?;
        let rows = statement.query_map(params![date.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (rule_set, rule_id, granted_at, until, reason) = row?;
            Ok(GrantedExtension {
                rule_set,
                rule_id,
                logical_date: date,
                granted_at: granted_at.parse()?,
                until: until.parse()?,
                reason,
            })
        })
        .collect()
    }

    fn started_on(&self, date: civil::Date) -> Result<Vec<RuleWindow>, StoreError> {
        let mut statement = self
            .conn
            .prepare("SELECT window FROM started_windows WHERE logical_date = ?1 ORDER BY rowid")?;
        let rows = statement.query_map(params![date.to_string()], |row| row.get::<_, String>(0))?;
        rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
    }

    fn shutdowns_on(&self, date: civil::Date) -> Result<Vec<ShutdownDone>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT at, detail FROM events WHERE logical_date = ?1 AND kind = ?2 ORDER BY at, id",
        )?;
        let kind = event_kind_text(EventKind::RuleShutdown);
        let rows = statement.query_map(params![date.to_string(), kind], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        rows.map(|row| {
            let (at, detail) = row?;
            let detail: ShutdownDetail =
                serde_json::from_str(detail.as_deref().unwrap_or_default())?;
            Ok(ShutdownDone {
                rule_id: detail.rule,
                logical_date: date,
                at: at.parse()?,
            })
        })
        .collect()
    }

    pub fn record_event(&self, event: &Event) -> Result<bool, StoreError> {
        let inserted = self.conn.execute(
            "INSERT OR IGNORE INTO events (at, logical_date, kind, key, detail)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                timestamp_text(event.at),
                event.logical_date.to_string(),
                event_kind_text(event.kind),
                event.key,
                event.detail
            ],
        )?;
        Ok(inserted > 0)
    }

    pub fn events_for(&self, logical_date: civil::Date) -> Result<Vec<Event>, StoreError> {
        let mut statement = self.conn.prepare(
            "SELECT at, kind, key, detail FROM events WHERE logical_date = ?1 ORDER BY at, id",
        )?;
        let rows = statement.query_map(params![logical_date.to_string()], event_row)?;
        rows.map(|row| {
            let (at, kind, key, detail) = row?;
            Ok(Event {
                at: at.parse()?,
                logical_date,
                kind: event_kind_from(&kind)?,
                key,
                detail,
            })
        })
        .collect()
    }
}

type EventColumns = (String, String, String, Option<String>);

fn event_row(row: &Row) -> rusqlite::Result<EventColumns> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}

fn timestamp_text(timestamp: Timestamp) -> String {
    timestamp.strftime("%Y-%m-%dT%H:%M:%S%.9fZ").to_string()
}
