pub const MIGRATIONS: &[&str] = &[
    "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT;
     CREATE TABLE config_versions (id INTEGER PRIMARY KEY, saved_at TEXT NOT NULL, effective_at TEXT NOT NULL,
       kind TEXT NOT NULL, text TEXT NOT NULL, superseded INTEGER NOT NULL DEFAULT 0) STRICT;
     CREATE TABLE extensions (id INTEGER PRIMARY KEY, rule_set TEXT NOT NULL, rule_id TEXT NOT NULL,
       logical_date TEXT NOT NULL, granted_at TEXT NOT NULL, until TEXT NOT NULL, reason TEXT NOT NULL) STRICT;
     CREATE TABLE started_windows (key TEXT NOT NULL, logical_date TEXT NOT NULL, window TEXT NOT NULL,
       PRIMARY KEY (key, logical_date)) STRICT;
     CREATE TABLE events (id INTEGER PRIMARY KEY, at TEXT NOT NULL, logical_date TEXT NOT NULL, kind TEXT NOT NULL,
       key TEXT NOT NULL, detail TEXT, UNIQUE (kind, key)) STRICT;",
    "ALTER TABLE config_versions RENAME TO config_versions_m1;
     CREATE TABLE config_versions (id INTEGER PRIMARY KEY, saved_at TEXT NOT NULL, text TEXT NOT NULL) STRICT;
     CREATE TABLE config_changes (id INTEGER PRIMARY KEY, version_id INTEGER NOT NULL REFERENCES config_versions (id),
       unit TEXT NOT NULL, kind TEXT NOT NULL, effective_at TEXT NOT NULL) STRICT;
     DELETE FROM started_windows;",
    "CREATE TABLE exceptions (rule_id TEXT NOT NULL, logical_date TEXT NOT NULL, requested_at TEXT NOT NULL,
       granted_at TEXT, reason TEXT, PRIMARY KEY (rule_id, logical_date)) STRICT;",
    "CREATE TABLE boot_announcements (boot_id TEXT NOT NULL, rule_key TEXT NOT NULL, starts_at TEXT NOT NULL,
       announced_at TEXT NOT NULL, PRIMARY KEY (boot_id, rule_key, starts_at)) STRICT;
     CREATE TABLE boot_notices (boot_id TEXT NOT NULL, key TEXT NOT NULL, at TEXT NOT NULL,
       PRIMARY KEY (boot_id, key, at)) STRICT;
     CREATE TABLE boot_shutdowns (boot_id TEXT NOT NULL, key TEXT NOT NULL, PRIMARY KEY (boot_id, key)) STRICT;
     CREATE TABLE shown_locks (identity TEXT PRIMARY KEY, label TEXT NOT NULL, logical_date TEXT NOT NULL,
       test INTEGER NOT NULL, ended INTEGER NOT NULL, boot_id TEXT NOT NULL, shutdown_on_screen TEXT,
       shut_down INTEGER NOT NULL) STRICT;",
];
