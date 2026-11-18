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
];
