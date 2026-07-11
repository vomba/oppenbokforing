CREATE INDEX IF NOT EXISTS idx_journal_lines_account
  ON journal_lines (account_id);

CREATE INDEX IF NOT EXISTS idx_vouchers_workspace_accounting_date
  ON vouchers (workspace_id, accounting_date DESC, id DESC);

CREATE INDEX IF NOT EXISTS idx_documents_workspace_created
  ON documents (workspace_id, created_at DESC);
