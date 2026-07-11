ALTER TABLE workspace_settings
ADD COLUMN dashboard_tour_completed INTEGER NOT NULL DEFAULT 0 CHECK (dashboard_tour_completed IN (0, 1));

-- Existing workspaces should not see the first-run tour again after upgrade.
UPDATE workspace_settings SET dashboard_tour_completed = 1;
