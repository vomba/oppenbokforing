CREATE TABLE IF NOT EXISTS counterparties (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  kind TEXT NOT NULL CHECK (kind IN ('customer', 'supplier')),
  name TEXT NOT NULL,
  email TEXT,
  org_number TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_counterparties_workspace
  ON counterparties (workspace_id, kind, name);

CREATE TABLE IF NOT EXISTS invoice_sequences (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  fiscal_year_id TEXT NOT NULL REFERENCES fiscal_years(id),
  prefix TEXT NOT NULL,
  next_number INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (workspace_id, fiscal_year_id)
);

CREATE TABLE IF NOT EXISTS invoices (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  counterparty_id TEXT NOT NULL REFERENCES counterparties(id),
  fiscal_year_id TEXT NOT NULL REFERENCES fiscal_years(id),
  invoice_number TEXT,
  status TEXT NOT NULL CHECK (status IN ('draft', 'issued', 'credited')),
  invoice_kind TEXT NOT NULL DEFAULT 'standard'
    CHECK (invoice_kind IN ('standard', 'credit_note')),
  source_invoice_id TEXT REFERENCES invoices(id),
  issue_date TEXT,
  due_date TEXT,
  total_ex_vat_minor INTEGER NOT NULL DEFAULT 0,
  total_vat_minor INTEGER NOT NULL DEFAULT 0,
  total_inc_vat_minor INTEGER NOT NULL DEFAULT 0,
  pdf_job_id TEXT,
  voucher_id TEXT REFERENCES vouchers(id),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_invoices_number_per_year
  ON invoices (workspace_id, fiscal_year_id, invoice_number)
  WHERE invoice_number IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_invoices_workspace_status
  ON invoices (workspace_id, status, created_at);

CREATE TABLE IF NOT EXISTS invoice_lines (
  id TEXT PRIMARY KEY,
  invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
  line_order INTEGER NOT NULL,
  description TEXT NOT NULL,
  quantity INTEGER NOT NULL DEFAULT 1 CHECK (quantity > 0),
  unit_price_minor INTEGER NOT NULL,
  vat_rate_bp INTEGER NOT NULL DEFAULT 0 CHECK (vat_rate_bp >= 0),
  account_number TEXT NOT NULL DEFAULT '3041',
  UNIQUE (invoice_id, line_order)
);

CREATE TABLE IF NOT EXISTS credit_notes (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  source_invoice_id TEXT NOT NULL REFERENCES invoices(id),
  credit_invoice_id TEXT NOT NULL REFERENCES invoices(id),
  reason TEXT,
  reversal_voucher_id TEXT REFERENCES vouchers(id),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (source_invoice_id, credit_invoice_id)
);
