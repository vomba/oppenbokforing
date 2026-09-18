CREATE TABLE IF NOT EXISTS invoice_snapshot_recovery_requirements (
  invoice_id TEXT PRIMARY KEY REFERENCES invoices(id),
  workspace_id TEXT NOT NULL REFERENCES workspaces(id),
  preserved_pdf_document_id TEXT REFERENCES documents(id),
  preserved_pdf_content_sha256 TEXT,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CHECK (
    (preserved_pdf_document_id IS NULL AND preserved_pdf_content_sha256 IS NULL)
    OR (preserved_pdf_document_id IS NOT NULL AND preserved_pdf_content_sha256 IS NOT NULL)
  )
);

CREATE INDEX IF NOT EXISTS idx_invoice_snapshot_recovery_requirements_workspace
  ON invoice_snapshot_recovery_requirements (workspace_id, invoice_id);

INSERT INTO invoice_snapshot_recovery_requirements (
  invoice_id,
  workspace_id,
  preserved_pdf_document_id,
  preserved_pdf_content_sha256
)
SELECT
  i.id,
  i.workspace_id,
  d.id,
  d.content_sha256
FROM invoices i
LEFT JOIN documents d
  ON d.workspace_id = i.workspace_id
 AND d.id = i.pdf_document_id
WHERE i.status IN ('issued', 'credited')
  AND NOT EXISTS (
    SELECT 1
    FROM invoice_issue_snapshots s
    WHERE s.workspace_id = i.workspace_id
      AND s.invoice_id = i.id
  );

CREATE TRIGGER IF NOT EXISTS invoice_snapshot_recovery_requirements_immutable_update
BEFORE UPDATE ON invoice_snapshot_recovery_requirements
BEGIN
  SELECT RAISE(ABORT, 'invoice snapshot recovery requirement is immutable');
END;

CREATE TRIGGER IF NOT EXISTS invoice_snapshot_recovery_requirements_immutable_delete
BEFORE DELETE ON invoice_snapshot_recovery_requirements
BEGIN
  SELECT RAISE(ABORT, 'invoice snapshot recovery requirement is immutable');
END;
