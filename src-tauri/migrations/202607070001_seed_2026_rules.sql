-- Seed 2026 compliance rules for Swedish enskild firma (engineering fixtures, not legal advice).
-- Applied to each workspace database on connect after workspace row exists.

INSERT OR IGNORE INTO rule_versions (
  id, tax_year, effective_from, effective_to, source_url, checksum, status
) VALUES (
  'rv-2026-active',
  2026,
  '2026-01-01',
  NULL,
  'https://www.skatteverket.se/servicelankar/otherlanguages/englishengelska/businessesandemployers/startingandrunningaswedishbusiness/registeringabusiness/incertaincasesyoudonotneedtoregisteryourbusinessforvat.4.6e1dd38d196873bc1e1cff.html',
  'sha256:2026-v1',
  'active'
);

INSERT OR IGNORE INTO tax_rules (id, rule_version_id, family, key, value_json) VALUES
  ('tr-2026-vat-threshold', 'rv-2026-active', 'vat', 'annual_turnover_threshold_minor', '12000000'),
  ('tr-2026-vat-exempt-text', 'rv-2026-active', 'vat', 'exemption_invoice_text_required', 'true'),
  ('tr-2026-f-skatt-wording', 'rv-2026-active', 'tax', 'invoice_must_mention_f_skatt', 'true'),
  ('tr-2026-fa-skatt-salary-ledger', 'rv-2026-active', 'tax', 'salary_income_in_business_ledger', 'false'),
  ('tr-2026-retention-years', 'rv-2026-active', 'bookkeeping', 'retention_years', '7'),
  ('tr-2026-k1-regime', 'rv-2026-active', 'year_end', 'accounting_regime', '"k1_simplified_annual_accounts"'),
  ('tr-2026-ne-required', 'rv-2026-active', 'year_end', 'ne_draft_required', 'true'),
  ('tr-2026-zero-vat-return', 'rv-2026-active', 'vat', 'registered_must_file_zero_return', 'true'),
  ('tr-2026-threshold-warning-ratio', 'rv-2026-active', 'vat', 'threshold_warning_ratio', '75');
