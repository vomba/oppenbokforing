-- Additive successor: retain every old payload and historical identity.
UPDATE rule_versions SET status='retired' WHERE tax_year=2026 AND status='active';
INSERT INTO rule_versions(id,tax_year,effective_from,effective_to,source_url,checksum,status)
VALUES('rv-2026-method-recognition-v2',2026,'2026-01-01','2026-12-31',
'https://www.bfn.se/wp-content/uploads/vl13-2-bokforing.pdf',
'sha256:5dbd4e6bd327f01112179ce325f086d9717f738d976065fbf4ec2268edd58cec','active');
INSERT INTO tax_rules(id,rule_version_id,family,key,value_json)
SELECT 'tr-method-v2-'||id,'rv-2026-method-recognition-v2',family,key,
CASE WHEN family='invoice_recognition' AND key='domestic_service25_exact'
THEN json_set(value_json,'$.validationId','invoice-recognition-se-service25-2026-exact-v2',
'$.evidenceDigest','5dbd4e6bd327f01112179ce325f086d9717f738d976065fbf4ec2268edd58cec')
ELSE value_json END
FROM tax_rules WHERE rule_version_id='rv-2026-domestic-service25-exact-v1';
INSERT INTO tax_rules(id,rule_version_id,family,key,value_json)
VALUES('tr-method-v2-cash-policy','rv-2026-method-recognition-v2','cash_recognition','domestic_service25_pdf_full',
'{"validationId":"cash-recognition-se-service25-2026-pdf-full-v1","evidenceDigest":"08d21e5dc4a055698b6a180e665141770bdb7f2c7325eea4e6d5bee4d54e7f7b","scope":{"accountingYear":2026,"accountingMethod":"cash_method","vatAccountingMethod":"bokslutsmetoden","currency":"SEK","transactionCase":"ordinary_completed_domestic_service","vatRateBp":2500,"priorPaymentReceived":false,"completionAndIssueInSameAccountingYear":true,"issueAndFullReceiptInSameAccountingYear":true,"reportingPeriod":"yearly","deadlineRegime":"annual_may_12","euTradeInAccountingYear":false,"annualTaxableBaseLimitMinor":100000000,"normalAnnualNetTurnoverCashMethodLimitMinor":300000000,"turnoverAndTaxableBaseAreSeparateFacts":true,"businessOrigin":"explicitly_reviewed_new_business_2026_zero_opening_no_unpaid_no_prior_unrecorded_activity","shortFirstYearIn2026Supported":true,"shortYearNormalTurnoverAdjustmentReviewRequired":true,"firstFinancialYearEndingOn":"2026-12-31","extendedFirstYearSupported":false,"receiptEvidence":"verified_retained_bank_pdf_plus_explicit_owner_received_facts_and_stable_entry_identity","cashReceiptsPerRetainedPdfHash":1,"roundingCoverage":"exact_vat_minor_units_only","cashVatApprovalSupported":false,"cashYearEndApprovalSupported":false},"calendar":{"accountingPeriod":"2026","yearWindowStart":"2026-01-01","yearWindowEnd":"2026-12-31","firstFinancialYearStart":"explicit_business_started_on_in_2026","firstFinancialYearEnd":"2026-12-31","filingYear":2027,"dueOn":"2027-05-12","applicability":"Annual VAT registered sole trader required to file an income tax return, no EU trade, annual taxable base at most SEK 1000000, no extension; reference calendar only, not permission to approve a cash VAT return or year-end."},"clockPolicy":{"calendarZone":"Europe/Stockholm","coveredEventYear":2026,"standardOffsetSeconds":3600,"summerOffsetSeconds":7200,"summerBeginsUtc":"2026-03-29T01:00:00Z","summerEndsUtc":"2026-10-25T01:00:00Z","purpose":"Compare the start of the candidate Swedish civil day in 2026 against a private production UTC clock. Source-backed finite-year transition data, not a general timezone implementation, tax-rate source or renderer clock.","sourceReferences":[{"id":"swedish-standard-time","title":"F\u00f6rordning (1979:988) om svensk normaltid","url":"https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/forordning-1979988-om-svensk-normaltid_sfs-1979-988/","passage":"Svensk normaltid is UTC increased by one hour.","retrievedOn":"2026-11-04"},{"id":"swedish-summer-time","title":"F\u00f6rordning (2001:127) om sommartid, 1\u20132 \u00a7\u00a7","url":"https://www.riksdagen.se/sv/dokument-och-lagar/dokument/svensk-forfattningssamling/forordning-2001127-om-sommartid_sfs-2001-127/","passage":"Summer time increases normal time by one hour; March transition is 02:00 normal time and October transition is 03:00 summer time.","retrievedOn":"2026-11-04"},{"id":"eu-summer-time-2026","title":"Commission schedule 2021/C 149/01","url":"https://eur-lex.europa.eu/legal-content/EN/TXT/HTML/?uri=CELEX%3A52021XC0427%2801%29","passage":"In 2026 the summer-time periods begin/end on 29 March and 25 October at 01:00 UTC.","retrievedOn":"2026-11-04"}],"futureCashIssueDateSupported":false,"futureCashReceiptDateSupported":false},"recognition":{"cashIssueIdentity":{"date":"2026-01-15","accountingDate":null,"numberedIssuedIdentity":true,"retainedIssuedPdfAndAuditProvenance":true,"voucherCountDelta":0,"journalLineCountDelta":0,"journalLines":[],"receivable1510MovementMinor":0,"revenue3041MovementMinor":0,"vatLiability2611MovementMinor":0,"bank1930MovementMinor":0,"notSettlementOrBankProof":true},"cashFullPdfReceipt":{"accountingDate":"2026-01-20","voucherCountDelta":1,"journalLineCountDelta":3,"journalLines":[{"account":"1930","debitMinor":1250000,"creditMinor":0},{"account":"3041","debitMinor":0,"creditMinor":1000000},{"account":"2611","debitMinor":0,"creditMinor":250000}],"debitTotalMinor":1250000,"creditTotalMinor":1250000,"receivable1510MovementMinor":0,"originalInvoiceAmountsAndIssueDateUnchanged":true,"separateReceiptIdentityStageAndProof":true,"cashVatApprovalSupported":false,"cashYearEndApprovalSupported":false}}}');

-- Composite keys enforce workspace boundaries, including planned receipt IDs.
CREATE UNIQUE INDEX cash_fk_invoices ON invoices(workspace_id,id);
CREATE UNIQUE INDEX cash_fk_years ON fiscal_years(workspace_id,id);
CREATE UNIQUE INDEX cash_fk_documents ON documents(workspace_id,id,content_sha256);
CREATE UNIQUE INDEX cash_fk_vouchers ON vouchers(workspace_id,id);
CREATE UNIQUE INDEX cash_fk_staging ON staged_transactions(workspace_id,id);
CREATE UNIQUE INDEX cash_fk_matches ON reconciliation_matches(workspace_id,id,invoice_id,voucher_id,staged_transaction_id);
CREATE TABLE cash_business_origins (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL UNIQUE REFERENCES workspaces(id),
 first_invoice_id TEXT NOT NULL,
 fiscal_year_id TEXT NOT NULL,
 facts_json TEXT NOT NULL,
 recorded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 UNIQUE(workspace_id,id),
 FOREIGN KEY(workspace_id,first_invoice_id) REFERENCES invoices(workspace_id,id),
 FOREIGN KEY(workspace_id,fiscal_year_id) REFERENCES fiscal_years(workspace_id,id)
);
CREATE TABLE cash_invoice_issues (
 invoice_id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 origin_id TEXT NOT NULL,
 facts_json TEXT NOT NULL,
 assessment_json TEXT NOT NULL,
 context_json TEXT NOT NULL,
 request_json TEXT NOT NULL,
 idempotency_key TEXT NOT NULL,
 result_json TEXT NOT NULL,
 net_minor INTEGER NOT NULL CHECK(net_minor>0),
 recorded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 UNIQUE(workspace_id,invoice_id),
 UNIQUE(workspace_id,idempotency_key),
 FOREIGN KEY(workspace_id,invoice_id) REFERENCES invoices(workspace_id,id),
 FOREIGN KEY(workspace_id,origin_id) REFERENCES cash_business_origins(workspace_id,id)
);
CREATE TABLE cash_receipt_recognitions (
 id TEXT PRIMARY KEY,
 workspace_id TEXT NOT NULL REFERENCES workspaces(id),
 invoice_id TEXT NOT NULL UNIQUE,
 origin_id TEXT NOT NULL,
 document_id TEXT NOT NULL,
 document_sha256 TEXT NOT NULL CHECK(length(document_sha256)=64),
 bank_identity TEXT NOT NULL CHECK(length(trim(bank_identity))>0),
 voucher_id TEXT NOT NULL UNIQUE,
 match_id TEXT NOT NULL UNIQUE,
 staged_transaction_id TEXT NOT NULL UNIQUE,
 idempotency_key TEXT NOT NULL,
 facts_json TEXT NOT NULL,
 assessment_json TEXT NOT NULL,
 context_json TEXT NOT NULL,
 request_json TEXT NOT NULL,
 result_json TEXT NOT NULL,
 recorded_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
 UNIQUE(workspace_id,document_sha256),
 UNIQUE(workspace_id,bank_identity),
 UNIQUE(workspace_id,idempotency_key),
 FOREIGN KEY(workspace_id,invoice_id) REFERENCES cash_invoice_issues(workspace_id,invoice_id),
 FOREIGN KEY(workspace_id,origin_id) REFERENCES cash_business_origins(workspace_id,id),
 FOREIGN KEY(workspace_id,document_id,document_sha256) REFERENCES documents(workspace_id,id,content_sha256),
 FOREIGN KEY(workspace_id,voucher_id) REFERENCES vouchers(workspace_id,id) DEFERRABLE INITIALLY DEFERRED,
 FOREIGN KEY(workspace_id,staged_transaction_id) REFERENCES staged_transactions(workspace_id,id) DEFERRABLE INITIALLY DEFERRED,
 FOREIGN KEY(workspace_id,match_id,invoice_id,voucher_id,staged_transaction_id)
 REFERENCES reconciliation_matches(workspace_id,id,invoice_id,voucher_id,staged_transaction_id) DEFERRABLE INITIALLY DEFERRED
);
CREATE TRIGGER cash_receipt_origin_guard BEFORE INSERT ON cash_receipt_recognitions
WHEN NOT EXISTS(SELECT 1 FROM cash_invoice_issues WHERE workspace_id=NEW.workspace_id AND invoice_id=NEW.invoice_id AND origin_id=NEW.origin_id)
BEGIN SELECT RAISE(ABORT,'cash receipt origin mismatch'); END;
CREATE TRIGGER cash_business_origins_immutable_update BEFORE UPDATE ON cash_business_origins
BEGIN SELECT RAISE(ABORT,'cash provenance is immutable'); END;
CREATE TRIGGER cash_business_origins_immutable_delete BEFORE DELETE ON cash_business_origins
BEGIN SELECT RAISE(ABORT,'cash provenance is immutable'); END;
CREATE TRIGGER cash_invoice_issues_immutable_update BEFORE UPDATE ON cash_invoice_issues
BEGIN SELECT RAISE(ABORT,'cash provenance is immutable'); END;
CREATE TRIGGER cash_invoice_issues_immutable_delete BEFORE DELETE ON cash_invoice_issues
BEGIN SELECT RAISE(ABORT,'cash provenance is immutable'); END;
CREATE TRIGGER cash_receipt_recognitions_immutable_update BEFORE UPDATE ON cash_receipt_recognitions
BEGIN SELECT RAISE(ABORT,'cash provenance is immutable'); END;
CREATE TRIGGER cash_receipt_recognitions_immutable_delete BEFORE DELETE ON cash_receipt_recognitions
BEGIN SELECT RAISE(ABORT,'cash provenance is immutable'); END;
