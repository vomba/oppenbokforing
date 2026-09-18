ALTER TABLE year_end_packages ADD COLUMN annual_accounts_bytes INTEGER;
ALTER TABLE year_end_packages ADD COLUMN annual_accounts_sha256 TEXT;
ALTER TABLE year_end_packages ADD COLUMN ne_draft_bytes INTEGER;
ALTER TABLE year_end_packages ADD COLUMN ne_draft_sha256 TEXT;
ALTER TABLE year_end_packages ADD COLUMN export_bytes INTEGER;
ALTER TABLE year_end_packages ADD COLUMN export_sha256 TEXT;
