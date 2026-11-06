-- Nullable by design: historical approvals remain readable; legacy drafts must
-- be regenerated and reviewed, never backfilled from unreviewed current data.
ALTER TABLE vat_returns ADD COLUMN review_revision TEXT;
ALTER TABLE vat_returns ADD COLUMN review_snapshot_json TEXT;
ALTER TABLE vat_returns ADD COLUMN review_snapshot_hash TEXT;
