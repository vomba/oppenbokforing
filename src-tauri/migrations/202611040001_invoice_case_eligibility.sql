-- Forward-only: preserve all historical payloads and references. The copied
-- engineering rules/calendars are NOT validated by this bounded case.
UPDATE rule_versions SET status = 'retired' WHERE id = 'rv-2026-active';
INSERT INTO rule_versions (id, tax_year, effective_from, effective_to, source_url, checksum, status)
VALUES ('rv-2026-domestic-service25-exact-v1', 2026, '2026-01-01', '2026-12-31',
 'https://www.bfn.se/wp-content/uploads/vl13-2-bokforing.pdf',
 'sha256:cb00f554e7ee3697b8461309e7184fd88b3a0aa70300f3f27ead9f6ca98b8594', 'active');

INSERT INTO tax_rules (id, rule_version_id, family, key, value_json)
SELECT 'tr-domestic-service25-v1-' || id, 'rv-2026-domestic-service25-exact-v1', family, key,
 CASE WHEN family = 'vat' AND key = 'filing_deadline_schedules'
 THEN json_set(value_json, '$.schedules.annual_may_12."2026"', '2027-05-12')
 ELSE value_json END
FROM tax_rules WHERE rule_version_id = 'rv-2026-active';

-- Exact typed policy: compared to the compiled reviewed manifest, not checksum.
INSERT INTO tax_rules (id, rule_version_id, family, key, value_json)
VALUES ('tr-domestic-service25-v1-policy', 'rv-2026-domestic-service25-exact-v1',
 'invoice_recognition', 'domestic_service25_exact',
 '{"validationId":"invoice-recognition-se-service25-2026-exact-v1","evidenceDigest":"cb00f554e7ee3697b8461309e7184fd88b3a0aa70300f3f27ead9f6ca98b8594","accountingYear":2026,"accountingMethod":"invoice_method","currency":"SEK","vatRateBp":2500,"revenueAccount":"3041","receivableAccount":"1510","outputVatAccount":"2611","annualTaxableBaseLimitMinor":100000000,"roundingCoverage":"exact_vat_minor_units_only","reportingPeriod":"yearly","deadlineRegime":"annual_may_12","periodKey":"2026","dueOn":"2027-05-12"}');

CREATE TABLE invoice_issue_validations (
 invoice_id TEXT PRIMARY KEY REFERENCES invoices(id),
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 facts_json TEXT NOT NULL,
 assessment_json TEXT NOT NULL,
 context_json TEXT NOT NULL,
 recorded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_invoice_issue_validations_workspace ON invoice_issue_validations(workspace_id, invoice_id);
CREATE TRIGGER invoice_issue_validations_immutable_update BEFORE UPDATE ON invoice_issue_validations
BEGIN SELECT RAISE(ABORT, 'invoice issue validation is immutable'); END;
CREATE TRIGGER invoice_issue_validations_immutable_delete BEFORE DELETE ON invoice_issue_validations
BEGIN SELECT RAISE(ABORT, 'invoice issue validation is immutable'); END;
-- No fabricated backfill: legacy invoices intentionally have no validation row.
