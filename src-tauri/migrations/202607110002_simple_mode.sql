ALTER TABLE workspace_settings
ADD COLUMN simple_mode INTEGER NOT NULL DEFAULT 1 CHECK (simple_mode IN (0, 1));

-- Preserve advanced UI for workspaces that existed before simple mode shipped.
UPDATE workspace_settings SET simple_mode = 0;
