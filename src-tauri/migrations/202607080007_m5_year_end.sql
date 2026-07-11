CREATE TABLE IF NOT EXISTS year_end_packages (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  fiscal_year_id TEXT NOT NULL REFERENCES fiscal_years(id),
  status TEXT NOT NULL CHECK (status IN ('draft', 'approved')),
  rule_version_id TEXT NOT NULL REFERENCES rule_versions(id),
  annual_accounts_path TEXT,
  ne_draft_path TEXT,
  export_path TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  approved_at TEXT,
  UNIQUE (workspace_id, fiscal_year_id)
);

CREATE INDEX IF NOT EXISTS idx_year_end_packages_workspace
  ON year_end_packages (workspace_id, fiscal_year_id);

CREATE TABLE IF NOT EXISTS ne_fields (
  id TEXT PRIMARY KEY,
  year_end_package_id TEXT NOT NULL REFERENCES year_end_packages(id) ON DELETE CASCADE,
  field_code TEXT NOT NULL,
  amount_minor INTEGER NOT NULL,
  source_type TEXT NOT NULL CHECK (source_type IN ('ledger', 'manual', 'adjustment')),
  source_ref TEXT,
  calculation_trace_id TEXT,
  manual_override INTEGER NOT NULL DEFAULT 0,
  UNIQUE (year_end_package_id, field_code)
);

CREATE INDEX IF NOT EXISTS idx_ne_fields_package
  ON ne_fields (year_end_package_id, field_code);
