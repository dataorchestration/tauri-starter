-- Initial schema. Add new migrations as 0002_*.sql and list them in db/mod.rs;
-- never edit a migration that has shipped.

CREATE TABLE users (
    id           TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    email        TEXT,
    avatar_path  TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

-- Key/value store. Typed app settings live under the key 'app' as one JSON
-- document (see db/settings.rs); anything else is free-form.
CREATE TABLE settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- In-app notification center (native OS notifications are fire-and-forget;
-- this table is what the UI lists).
CREATE TABLE notifications (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    title      TEXT NOT NULL,
    body       TEXT NOT NULL DEFAULT '',
    kind       TEXT NOT NULL DEFAULT 'info',
    read       INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_notifications_read_created ON notifications (read, created_at DESC);
