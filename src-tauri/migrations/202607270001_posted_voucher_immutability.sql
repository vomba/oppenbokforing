-- Posted voucher immutability (domain invariant 7): block UPDATE/DELETE once posted.
CREATE TRIGGER IF NOT EXISTS vouchers_no_update_when_posted
BEFORE UPDATE ON vouchers
WHEN OLD.status = 'posted'
BEGIN
  SELECT RAISE(ABORT, 'Posted vouchers cannot be updated');
END;

CREATE TRIGGER IF NOT EXISTS vouchers_no_delete_when_posted
BEFORE DELETE ON vouchers
WHEN OLD.status = 'posted'
BEGIN
  SELECT RAISE(ABORT, 'Posted vouchers cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS journal_lines_no_update_for_posted_voucher
BEFORE UPDATE ON journal_lines
WHEN EXISTS (
  SELECT 1 FROM vouchers v
  WHERE v.id = OLD.voucher_id AND v.status = 'posted'
)
BEGIN
  SELECT RAISE(ABORT, 'Journal lines of posted vouchers cannot be updated');
END;

CREATE TRIGGER IF NOT EXISTS journal_lines_no_delete_for_posted_voucher
BEFORE DELETE ON journal_lines
WHEN EXISTS (
  SELECT 1 FROM vouchers v
  WHERE v.id = OLD.voucher_id AND v.status = 'posted'
)
BEGIN
  SELECT RAISE(ABORT, 'Journal lines of posted vouchers cannot be deleted');
END;

CREATE TRIGGER IF NOT EXISTS audit_events_no_update
BEFORE UPDATE ON audit_events
BEGIN
  SELECT RAISE(ABORT, 'Audit events are append-only');
END;

CREATE TRIGGER IF NOT EXISTS audit_events_no_delete
BEFORE DELETE ON audit_events
BEGIN
  SELECT RAISE(ABORT, 'Audit events are append-only');
END;

-- Hot path for verify_balanced_tx / voucher_get / VAT aggregates.
CREATE INDEX IF NOT EXISTS idx_journal_lines_voucher
  ON journal_lines (voucher_id);
