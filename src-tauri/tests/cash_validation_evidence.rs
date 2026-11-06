//! Candidate EVIDENCE consistency only: these tests neither execute proposed cash
//! transitions nor establish runtime coverage, rule activation or bank authenticity.
use chrono::{Datelike, NaiveDate};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const CASH: &str = include_str!("../../fixtures/accounting-validation/2026-cash-pdf-full.json");
const INVOICE_V1: &str =
    include_str!("../../fixtures/accounting-validation/2026-domestic-service25-exact.json");
const INVOICE_V2: &str =
    include_str!("../../fixtures/accounting-validation/2026-domestic-service25-exact-v2.json");
const V1_SHA256: &str = "cb00f554e7ee3697b8461309e7184fd88b3a0aa70300f3f27ead9f6ca98b8594";

fn cash() -> Value {
    serde_json::from_str(CASH).unwrap()
}

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn amount(value: &Value) -> i128 {
    i128::from(value.as_i64().unwrap())
}

#[test]
fn candidate_amount_evidence_is_independently_exact_per_line_and_i64_bounded() {
    let evidence = cash();
    let examples = evidence["expected"].as_array().unwrap();
    assert_eq!(examples.len(), 12);
    let mut ids = BTreeSet::new();
    for example in examples {
        let id = example["id"].as_str().unwrap();
        assert!(ids.insert(id), "duplicate {id}");
        let nets: Vec<i128> = example["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| {
                amount(&line["quantity"])
                    .checked_mul(amount(&line["unitPriceMinor"]))
                    .unwrap()
            })
            .collect();
        assert!(!nets.is_empty(), "{id}");
        let sum = |values: &[i128]| {
            values
                .iter()
                .try_fold(0_i128, |sum, n| sum.checked_add(*n))
                .unwrap()
        };
        let net = sum(&nets);
        let vats: Vec<i128> = nets.iter().map(|n| n / 4).collect();
        let vat = sum(&vats);
        let gross = net.checked_add(vat).unwrap();
        let fractional = nets.iter().any(|n| n % 4 != 0);
        let out_of_range = nets
            .iter()
            .chain(vats.iter())
            .chain([net, vat, gross].iter())
            .any(|n| i64::try_from(*n).is_err());
        match example["blocked"].as_str() {
            Some("rounding_not_covered") => assert!(fractional && !out_of_range, "{id}"),
            Some("amount_out_of_range") => assert!(out_of_range, "{id}"),
            None => {
                assert!(!fractional && !out_of_range, "{id}");
                assert_eq!(net, amount(&example["netMinor"]), "{id}");
                assert_eq!(vat, amount(&example["vatMinor"]), "{id}");
                assert_eq!(gross, amount(&example["grossMinor"]), "{id}");
            }
            Some(other) => panic!("unrecognized evidence outcome {other}: {id}"),
        }
        if id == "fractions-cancel-only-in-total" {
            assert_eq!(net % 4, 0);
            assert!(
                fractional,
                "total exactness cannot replace per-line exactness"
            );
        }
    }
    let example = &evidence["example"];
    assert_eq!(example["lines"], examples[0]["lines"]);
    for key in ["netMinor", "vatMinor", "grossMinor"] {
        assert_eq!(example[key], examples[0][key], "{key}");
    }
}

#[test]
fn literal_issue_and_full_receipt_evidence_have_distinct_dates_and_no_receivable() {
    let evidence = cash();
    let example = &evidence["example"];
    assert_eq!(example["completedOn"], "2026-01-10");
    assert_eq!(example["issueDate"], "2026-01-15");
    assert_eq!(example["receiptDate"], "2026-01-20");
    let issue = &evidence["recognition"]["cashIssueIdentity"];
    assert_eq!(issue["date"], example["issueDate"]);
    assert!(issue["accountingDate"].is_null());
    assert_eq!(issue["journalLines"], json!([]));
    for key in [
        "voucherCountDelta",
        "journalLineCountDelta",
        "receivable1510MovementMinor",
        "revenue3041MovementMinor",
        "vatLiability2611MovementMinor",
        "bank1930MovementMinor",
    ] {
        assert_eq!(issue[key], 0, "{key}");
    }
    assert_eq!(issue["numberedIssuedIdentity"], true);
    assert_eq!(issue["retainedIssuedPdfAndAuditProvenance"], true);
    let receipt = &evidence["recognition"]["cashFullPdfReceipt"];
    assert_eq!(receipt["accountingDate"], example["receiptDate"]);
    assert_ne!(receipt["accountingDate"], issue["date"]);
    assert_eq!(receipt["voucherCountDelta"], 1);
    assert_eq!(receipt["journalLineCountDelta"], 3);
    assert_eq!(
        receipt["journalLines"],
        json!([
            {"account":"1930", "debitMinor":1250000, "creditMinor":0},
            {"account":"3041", "debitMinor":0, "creditMinor":1000000},
            {"account":"2611", "debitMinor":0, "creditMinor":250000}
        ])
    );
    let lines = receipt["journalLines"].as_array().unwrap();
    let debit: i128 = lines.iter().map(|line| amount(&line["debitMinor"])).sum();
    let credit: i128 = lines.iter().map(|line| amount(&line["creditMinor"])).sum();
    assert_eq!(debit, credit);
    assert_eq!(debit, amount(&example["grossMinor"]));
    assert_eq!(debit, amount(&receipt["debitTotalMinor"]));
    assert_eq!(credit, amount(&receipt["creditTotalMinor"]));
    assert_eq!(receipt["receivable1510MovementMinor"], 0);
    assert!(lines.iter().all(|line| line["account"] != "1510"));
    let facts = &example["receipt"];
    assert_eq!(facts["actualReceivedAmountMinor"], example["grossMinor"]);
    assert_eq!(facts["actualReceivedOn"], example["receiptDate"]);
    assert_eq!(facts["actualReceivedConfirmed"], true);
    assert_eq!(facts["sourceCurrency"], "SEK");
    assert_eq!(facts["sourceKind"], "retained_external_bank_pdf");
}

#[test]
fn eligibility_evidence_keeps_three_million_turnover_separate_from_one_million_vat_base() {
    let evidence = cash();
    let scope = &evidence["scope"];
    assert_eq!(
        scope["normalAnnualNetTurnoverCashMethodLimitMinor"],
        3_000_000_i64 * 100
    );
    assert_eq!(scope["annualTaxableBaseLimitMinor"], 1_000_000_i64 * 100);
    assert_eq!(scope["turnoverAndTaxableBaseAreSeparateFacts"], true);
    let examples = evidence["eligibilityExpected"].as_array().unwrap();
    assert_eq!(examples.len(), 5);
    let mut ids = BTreeSet::new();
    for example in examples {
        let id = example["id"].as_str().unwrap();
        assert!(ids.insert(id), "duplicate {id}");
        let turnover = example["normalAnnualNetTurnoverMinor"]
            .as_i64()
            .map(|n| n <= 300_000_000);
        let base = example["annualTaxableBaseMinor"]
            .as_i64()
            .map(|n| n <= 100_000_000);
        assert_eq!(example["cashTurnoverWithinLimit"], json!(turnover), "{id}");
        assert_eq!(example["annualBaseWithinLimit"], json!(base), "{id}");
        let blocked = match (turnover, base) {
            (Some(true), Some(true)) => None,
            (Some(false), _) => Some("outside_cash_turnover_limit"),
            (_, Some(false)) => Some("outside_annual_vat_base_limit"),
            (None, _) => Some("cash_eligibility_unknown_not_inferred_from_annual_base"),
            (_, None) => Some("annual_eligibility_unknown_not_inferred_from_cash_turnover"),
        };
        assert_eq!(example["blocked"].as_str(), blocked, "{id}");
    }
}

#[test]
fn first_year_evidence_requires_actual_2026_start_december_end_and_short_year_review() {
    let evidence = cash();
    let scope = &evidence["scope"];
    assert_eq!(scope["shortFirstYearIn2026Supported"], true);
    assert_eq!(
        scope["shortYearNormalTurnoverAdjustmentReviewRequired"],
        true
    );
    assert_eq!(scope["firstFinancialYearEndingOn"], "2026-12-31");
    assert_eq!(scope["extendedFirstYearSupported"], false);
    let examples = evidence["firstYearExpected"].as_array().unwrap();
    assert_eq!(examples.len(), 9);
    let mut ids = BTreeSet::new();
    for example in examples {
        let id = example["id"].as_str().unwrap();
        assert!(ids.insert(id), "duplicate {id}");
        let start = example["businessStartedOn"].as_str().map(date);
        let completed = date(example["completedOn"].as_str().unwrap());
        let eligible = start.is_some_and(|start| {
            start.year() == 2026
                && start <= completed
                && completed.year() == 2026
                && example["firstFinancialYearEndsOn"] == "2026-12-31"
                && (start == date("2026-01-01")
                    || example["shortYearTurnoverAdjustmentReviewed"] == true)
        });
        assert_eq!(example["eligible"], json!(eligible), "{id}");
    }
    // Pin the full/short positive and negative inventory; none executes runtime guards.
    for (id, eligible) in [
        ("full-calendar", true),
        ("mid-year-december-end", true),
        ("late-year-december-end", true),
        ("short-year-adjustment-unknown", false),
        ("extended-first-year", false),
        ("prior-year-start", false),
        ("non-december-end", false),
        ("unknown-first-year-end", false),
        ("start-after-supply", false),
    ] {
        let example = examples.iter().find(|e| e["id"] == id).unwrap();
        assert_eq!(example["eligible"], json!(eligible), "{id}");
    }
}

#[test]
fn invoice_successor_inherits_every_published_field_except_its_two_identities() {
    let v1: Value = serde_json::from_str(INVOICE_V1).unwrap();
    let v2: Value = serde_json::from_str(INVOICE_V2).unwrap();
    let digest = format!("{:x}", Sha256::digest(INVOICE_V1.as_bytes()));
    assert_eq!(digest, V1_SHA256, "published v1 raw bytes are immutable");
    for (key, value) in v1.as_object().unwrap() {
        if key != "validationId" && key != "ruleVersionId" {
            assert_eq!(&v2[key], value, "inherited field {key}");
        }
    }
    assert_eq!(
        v2["validationId"],
        "invoice-recognition-se-service25-2026-exact-v2"
    );
    assert_eq!(v2["ruleVersionId"], "rv-2026-method-recognition-v2");
    assert_eq!(v2["ruleVersionId"], cash()["ruleVersionId"]);
    let lineage = &v2["derivedFrom"];
    assert_eq!(lineage["validationId"], v1["validationId"]);
    assert_eq!(lineage["ruleVersionId"], v1["ruleVersionId"]);
    assert_eq!(
        lineage["path"],
        "fixtures/accounting-validation/2026-domestic-service25-exact.json"
    );
    assert_eq!(lineage["sha256"], digest);
    assert_eq!(lineage["relationship"], "compatibility_lineage_only");
    assert_eq!(lineage["expectationsAndSourcesUnchanged"], true);
    assert_eq!(lineage["newlyValidatedInvoiceScope"], false);
}

#[test]
fn named_oracle_inventory_retains_one_receipt_per_pdf_boundary_without_executing_it() {
    let evidence = cash();
    assert_eq!(evidence["scope"]["cashReceiptsPerRetainedPdfHash"], 1);
    let scenarios = evidence["oracleScenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 26);
    let mut ids = BTreeSet::new();
    for scenario in scenarios {
        let id = scenario["id"].as_str().unwrap();
        assert!(
            !id.is_empty() && ids.insert(id),
            "missing or duplicate scenario ID"
        );
        assert!(!scenario["operation"].as_str().unwrap().is_empty(), "{id}");
        assert!(
            scenario["blocked"].is_string() || scenario["expectation"].is_string(),
            "{id}"
        );
    }
    let pdf_reuse = scenarios
        .iter()
        .find(|s| s["id"] == "pdf-content-reuse-with-other-identity")
        .unwrap();
    assert_eq!(pdf_reuse["operation"], "cash_full_pdf_receipt");
    assert_eq!(pdf_reuse["blocked"], "one_cash_receipt_per_retained_pdf");
    // Presence is a coverage inventory check, NOT a passing test of these cases.
    for id in [
        "issue-identity-only",
        "full-actual-receipt",
        "second-proven-cash-invoice",
        "second-distinct-receipt",
        "exact-replay",
        "duplicate-source-new-key-or-invoice",
        "cached-replay-changed-inputs",
        "cached-result-missing-or-tampered-pdf",
        "missing-owner-origin-review",
        "false-origin-review",
        "actual-history-contradiction",
        "missing-or-inconsistent-method-registration",
        "separate-turnover-limits",
        "year-or-calendar",
        "invalid-event-sequence",
        "actual-amount-not-full",
        "currency-or-received-not-proven",
        "invalid-bank-pdf",
        "stale-or-inapplicable-proof",
        "receipt-lock-and-atomicity",
        "method-change-after-identity-issue",
        "cash-csv-posting",
        "cash-unsupported-lifecycle",
        "cash-approvals-even-fully-paid",
        "unpaid-closing",
    ] {
        assert!(ids.contains(id), "missing oracle scenario {id}");
    }
}
