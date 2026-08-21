ALTER TABLE vat_profiles ADD COLUMN vat_filing_deadline_regime TEXT
  CHECK (vat_filing_deadline_regime IS NULL OR vat_filing_deadline_regime IN (
    'annual_may_12', 'annual_feb_26', 'quarterly_12', 'monthly_12', 'monthly_26'
  ));

-- Official ordinary 2026 schedules only. The calendar intentionally returns an
-- undated setup task when no matching versioned schedule is available.
INSERT OR IGNORE INTO tax_rules (id, rule_version_id, family, key, value_json) VALUES
  (
    'tr-2026-vat-filing-deadline-schedules',
    'rv-2026-active',
    'vat',
    'filing_deadline_schedules',
    '{"sourceUrl":"https://www.skatteverket.se/foretag/moms/deklareramoms/narskajagdeklareramoms.4.6d02084411db6e252fe80008988.html","schedules":{"annual_may_12":{"2026":"2026-05-12"},"annual_feb_26":{"2026":"2026-02-26"},"quarterly_12":{"2026-Q1":"2026-05-12","2026-Q2":"2026-08-17","2026-Q3":"2026-11-12","2026-Q4":"2027-02-12"},"monthly_12":{"2026-M01":"2026-03-12","2026-M02":"2026-04-12","2026-M03":"2026-05-12","2026-M04":"2026-06-12","2026-M05":"2026-07-12","2026-M06":"2026-08-17","2026-M07":"2026-09-12","2026-M08":"2026-10-12","2026-M09":"2026-11-12","2026-M10":"2026-12-12","2026-M11":"2027-01-17","2026-M12":"2027-02-12"},"monthly_26":{"2026-M01":"2026-02-26","2026-M02":"2026-03-26","2026-M03":"2026-04-26","2026-M04":"2026-05-26","2026-M05":"2026-06-26","2026-M06":"2026-07-26","2026-M07":"2026-08-26","2026-M08":"2026-09-26","2026-M09":"2026-10-26","2026-M10":"2026-11-26","2026-M11":"2026-12-27","2026-M12":"2027-01-26"}}}'
  ),
  (
    'tr-2026-income-declaration-deadline',
    'rv-2026-active',
    'year_end',
    'income_declaration_deadline',
    '{"sourceUrl":"https://www.skatteverket.se/privat/deklaration/datumfordeklarationen2026.4.1997e70d1848dabbac95d72.html","dueOn":"2026-05-04"}'
  );

-- The ordinary 2026 declaration deadline concerns the completed 2025 fiscal
-- year. These year-end rule values make the linked 2025 package workflow
-- executable without treating the 2026 deadline as a 2026 fiscal-year task.
INSERT OR IGNORE INTO rule_versions (
  id, tax_year, effective_from, effective_to, source_url, checksum, status
) VALUES (
  'rv-2025-year-end',
  2025,
  '2025-01-01',
  '2025-12-31',
  'https://www.skatteverket.se/privat/deklaration/datumfordeklarationen2026.4.1997e70d1848dabbac95d72.html',
  'sha256:2025-year-end-v1',
  'active'
);

INSERT OR IGNORE INTO tax_rules (id, rule_version_id, family, key, value_json) VALUES
  ('tr-2025-k1-regime', 'rv-2025-year-end', 'year_end', 'accounting_regime', '"k1_simplified_annual_accounts"'),
  ('tr-2025-ne-required', 'rv-2025-year-end', 'year_end', 'ne_draft_required', 'true');
