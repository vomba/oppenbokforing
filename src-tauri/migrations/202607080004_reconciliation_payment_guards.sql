CREATE UNIQUE INDEX IF NOT EXISTS idx_reconciliation_one_payment_per_invoice
  ON reconciliation_matches (workspace_id, invoice_id)
  WHERE match_kind = 'invoice_payment' AND invoice_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_vouchers_one_reconciliation_per_invoice
  ON vouchers (workspace_id, source_id)
  WHERE source_type = 'reconciliation' AND source_id IS NOT NULL;
