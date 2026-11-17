use chrono::{Datelike, NaiveDate, Weekday};
use serde_json::Value;

const EVIDENCE: &str =
    include_str!("../../fixtures/accounting-validation/2026-domestic-service25-exact.json");

#[test]
fn independently_derived_exact_vat_examples_are_arithmetically_consistent() {
    let evidence: Value = serde_json::from_str(EVIDENCE).unwrap();
    for example in evidence["expected"].as_array().unwrap() {
        let id = example["id"].as_str().unwrap();
        let nets: Vec<i128> = example["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| {
                i128::from(line["quantity"].as_i64().unwrap())
                    * i128::from(line["unitPriceMinor"].as_i64().unwrap())
            })
            .collect();
        let net: i128 = nets.iter().sum();
        let fractional = nets.iter().any(|amount| amount % 4 != 0);
        let vat: i128 = nets.iter().map(|amount| amount / 4).sum();
        let gross = net + vat;
        let out_of_range = nets.iter().any(|amount| *amount > i128::from(i64::MAX))
            || net > i128::from(i64::MAX)
            || gross > i128::from(i64::MAX);
        match example["blocked"].as_str() {
            Some("rounding_not_covered") => assert!(fractional, "{id}"),
            Some("amount_out_of_range") => assert!(out_of_range, "{id}"),
            None => {
                assert!(!fractional && !out_of_range, "{id}");
                assert_eq!(
                    net,
                    i128::from(example["netMinor"].as_i64().unwrap()),
                    "{id}"
                );
                assert_eq!(
                    vat,
                    i128::from(example["vatMinor"].as_i64().unwrap()),
                    "{id}"
                );
                assert_eq!(
                    gross,
                    i128::from(example["grossMinor"].as_i64().unwrap()),
                    "{id}"
                );
            }
            Some(other) => panic!("unrecognized evidence outcome {other}: {id}"),
        }
    }
}

#[test]
fn evidence_separates_recognition_from_receipt_and_accounting_year_from_filing_year() {
    let evidence: Value = serde_json::from_str(EVIDENCE).unwrap();
    let recognition = &evidence["recognition"];
    let issued =
        NaiveDate::parse_from_str(recognition["issueDate"].as_str().unwrap(), "%Y-%m-%d").unwrap();
    assert_eq!(issued.year(), 2026);
    assert_eq!(recognition["issueDate"], recognition["accountingDate"]);
    assert_eq!(recognition["completedOn"], recognition["accountingDate"]);
    assert_eq!(
        recognition["debitMinor"].as_i64().unwrap(),
        recognition["creditRevenueMinor"].as_i64().unwrap()
            + recognition["creditVatMinor"].as_i64().unwrap()
    );
    assert_eq!(recognition["bankMovementMinor"].as_i64(), Some(0));
    let calendar = &evidence["calendar"];
    let due = NaiveDate::parse_from_str(calendar["dueOn"].as_str().unwrap(), "%Y-%m-%d").unwrap();
    assert_eq!(calendar["accountingPeriod"].as_str(), Some("2026"));
    assert_eq!(
        due,
        NaiveDate::from_ymd_opt(issued.year() + 1, 5, 12).unwrap()
    );
    assert_eq!(due.weekday(), Weekday::Wed);
    assert_eq!(calendar["filingYear"].as_i64(), Some(i64::from(due.year())));
}
