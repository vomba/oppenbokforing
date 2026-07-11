ALTER TABLE vouchers ADD COLUMN accounting_date TEXT;

UPDATE vouchers
SET accounting_date = date(posted_at)
WHERE accounting_date IS NULL;
