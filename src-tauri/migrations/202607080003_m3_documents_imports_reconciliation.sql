CREATE TABLE IF NOT EXISTS documents (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  object_path TEXT NOT NULL,
  content_sha256 TEXT NOT NULL,
  mime_type TEXT NOT NULL,
  original_filename TEXT NOT NULL,
  retention_years INTEGER NOT NULL DEFAULT 7 CHECK (retention_years > 0),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (workspace_id, content_sha256)
);

ALTER TABLE vouchers ADD COLUMN document_id TEXT REFERENCES documents(id);
ALTER TABLE vouchers ADD COLUMN no_document_reason TEXT;

CREATE TABLE IF NOT EXISTS csv_imports (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  source_document_id TEXT REFERENCES documents(id),
  status TEXT NOT NULL CHECK (status IN ('created', 'parsed', 'failed')),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS staged_transactions (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  csv_import_id TEXT REFERENCES csv_imports(id),
  transaction_date TEXT NOT NULL,
  description TEXT NOT NULL,
  amount_minor INTEGER NOT NULL,
  status TEXT NOT NULL DEFAULT 'staged'
    CHECK (status IN ('staged', 'matched', 'ignored')),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS reconciliation_matches (
  id TEXT PRIMARY KEY,
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  staged_transaction_id TEXT NOT NULL REFERENCES staged_transactions(id),
  match_kind TEXT NOT NULL,
  invoice_id TEXT REFERENCES invoices(id),
  voucher_id TEXT REFERENCES vouchers(id),
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE (staged_transaction_id)
);

CREATE INDEX IF NOT EXISTS idx_documents_workspace_hash
  ON documents (workspace_id, content_sha256);

CREATE INDEX IF NOT EXISTS idx_staged_transactions_workspace_status
  ON staged_transactions (workspace_id, status, created_at);

