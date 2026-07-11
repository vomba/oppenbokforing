CREATE TABLE IF NOT EXISTS fiscal_periods (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  fiscal_year_id TEXT NOT NULL REFERENCES fiscal_years(id),
  period_key TEXT NOT NULL,
  starts_on TEXT NOT NULL,
  ends_on TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'open'
    CHECK (status IN ('open', 'locked')),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (workspace_id, period_key)
);

CREATE INDEX IF NOT EXISTS idx_fiscal_periods_workspace
  ON fiscal_periods (workspace_id, status, starts_on);

CREATE TABLE IF NOT EXISTS vat_codes (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  code TEXT NOT NULL,
  rate_bp INTEGER NOT NULL,
  output_box TEXT,
  input_box TEXT,
  deductible INTEGER NOT NULL DEFAULT 1,
  UNIQUE (workspace_id, code)
);

CREATE TABLE IF NOT EXISTS vat_returns (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  fiscal_period_id TEXT NOT NULL REFERENCES fiscal_periods(id),
  status TEXT NOT NULL CHECK (status IN ('draft', 'approved')),
  rule_version_id TEXT NOT NULL REFERENCES rule_versions(id),
  export_path TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  approved_at TEXT,
  UNIQUE (workspace_id, fiscal_period_id)
);

CREATE TABLE IF NOT EXISTS vat_return_boxes (
  id TEXT PRIMARY KEY,
  vat_return_id TEXT NOT NULL REFERENCES vat_returns(id) ON DELETE CASCADE,
  box_code TEXT NOT NULL,
  amount_minor INTEGER NOT NULL,
  source_query_hash TEXT,
  UNIQUE (vat_return_id, box_code)
);

CREATE TABLE IF NOT EXISTS tax_reservations (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  kind TEXT NOT NULL CHECK (kind IN ('vat', 'preliminary_tax')),
  amount_minor INTEGER NOT NULL DEFAULT 0,
  confidence TEXT NOT NULL DEFAULT 'estimated',
  calculation_trace_id TEXT,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (workspace_id, kind)
);
