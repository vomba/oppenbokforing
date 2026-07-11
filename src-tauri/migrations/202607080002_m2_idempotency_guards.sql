ALTER TABLE local_jobs ADD COLUMN idempotency_key TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_local_jobs_idempotency
  ON local_jobs (workspace_id, job_type, idempotency_key)
  WHERE idempotency_key IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_credit_notes_one_per_source
  ON credit_notes (source_invoice_id);
