CREATE TABLE IF NOT EXISTS workspace_settings (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL UNIQUE REFERENCES workspaces(id),
  locale TEXT NOT NULL DEFAULT 'en' CHECK (locale IN ('en', 'sv')),
  updater_enabled INTEGER NOT NULL DEFAULT 0 CHECK (updater_enabled IN (0, 1)),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
