CREATE TABLE IF NOT EXISTS invoice_issue_snapshots (
  invoice_id TEXT PRIMARY KEY REFERENCES invoices(id),
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  business_name TEXT NOT NULL,
  owner_name TEXT NOT NULL,
  tax_status TEXT NOT NULL,
  vat_status TEXT NOT NULL,
  rule_version_id TEXT NOT NULL REFERENCES rule_versions(id),
  tax_year INTEGER NOT NULL,
  source_url TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_invoice_issue_snapshots_workspace
  ON invoice_issue_snapshots (workspace_id, invoice_id);

CREATE TRIGGER IF NOT EXISTS invoice_issue_snapshots_immutable_update
BEFORE UPDATE ON invoice_issue_snapshots
BEGIN
  SELECT RAISE(ABORT, 'invoice issue snapshot is immutable');
END;

CREATE TRIGGER IF NOT EXISTS invoice_issue_snapshots_immutable_delete
BEFORE DELETE ON invoice_issue_snapshots
BEGIN
  SELECT RAISE(ABORT, 'invoice issue snapshot is immutable');
END;
