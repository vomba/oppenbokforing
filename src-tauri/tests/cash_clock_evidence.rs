//! Candidate calendar/clock EVIDENCE consistency only: these tests do not
//! exercise production clocks, cash transitions, rule activation or approvals.
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, TimeZone, Utc, Weekday};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const CASH: &str = include_str!("../../fixtures/accounting-validation/2026-cash-pdf-full.json");

fn cash() -> Value {
    serde_json::from_str(CASH).unwrap()
}

fn date(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap()
}

fn utc(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .unwrap()
        .with_timezone(&Utc)
}

#[test]
fn annual_calendar_is_a_may_2027_reference_not_cash_approval_evidence() {
    let evidence = cash();
    let calendar = &evidence["calendar"];
    let year = evidence["scope"]["accountingYear"].as_i64().unwrap() as i32;
    assert_eq!(year, 2026);
    assert_eq!(calendar["accountingPeriod"], "2026");
    assert_eq!(calendar["yearWindowStart"], "2026-01-01");
    assert_eq!(calendar["yearWindowEnd"], "2026-12-31");
    assert_eq!(
        calendar["firstFinancialYearStart"],
        "explicit_business_started_on_in_2026"
    );
    assert_eq!(calendar["firstFinancialYearEnd"], "2026-12-31");
    let due = date(calendar["dueOn"].as_str().unwrap());
    assert_eq!(due, NaiveDate::from_ymd_opt(year + 1, 5, 12).unwrap());
    assert_eq!(due.weekday(), Weekday::Wed);
    assert_eq!(calendar["filingYear"], due.year());
    for location in [
        &evidence["scope"],
        &evidence["recognition"]["cashFullPdfReceipt"],
    ] {
        assert_eq!(location["cashVatApprovalSupported"], false);
        assert_eq!(location["cashYearEndApprovalSupported"], false);
    }
}

#[test]
fn clock_evidence_matches_only_the_source_backed_2026_utc_transition_instants() {
    let evidence = cash();
    let policy = &evidence["clockPolicy"];
    assert_eq!(policy["calendarZone"], "Europe/Stockholm");
    assert_eq!(policy["coveredEventYear"], 2026);
    assert_eq!(policy["standardOffsetSeconds"], 3600);
    assert_eq!(policy["summerOffsetSeconds"], 7200);
    assert_eq!(policy["summerBeginsUtc"], "2026-03-29T01:00:00Z");
    assert_eq!(policy["summerEndsUtc"], "2026-10-25T01:00:00Z");
    let sources: BTreeSet<_> = policy["sourceReferences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        sources,
        BTreeSet::from([
            "swedish-standard-time",
            "swedish-summer-time",
            "eu-summer-time-2026"
        ])
    );
    let begin = utc(policy["summerBeginsUtc"].as_str().unwrap());
    let end = utc(policy["summerEndsUtc"].as_str().unwrap());
    let examples = evidence["clockExpected"].as_array().unwrap();
    assert_eq!(examples.len(), 8);
    let mut ids = BTreeSet::new();
    for example in examples {
        let id = example["id"].as_str().unwrap();
        assert!(ids.insert(id), "duplicate {id}");
        let instant = utc(example["utc"].as_str().unwrap());
        assert_eq!(instant.year(), 2026, "finite-year evidence only");
        let offset = if instant >= begin && instant < end {
            7200
        } else {
            3600
        };
        let local = instant.with_timezone(&FixedOffset::east_opt(offset).unwrap());
        assert_eq!(
            local.date_naive(),
            date(example["swedishDate"].as_str().unwrap()),
            "{id}"
        );
    }
}

#[test]
fn future_day_evidence_uses_independently_derived_2026_local_midnights() {
    let evidence = cash();
    assert_eq!(
        evidence["clockPolicy"]["futureCashIssueDateSupported"],
        false
    );
    assert_eq!(
        evidence["clockPolicy"]["futureCashReceiptDateSupported"],
        false
    );
    let examples = evidence["futureDayExpected"].as_array().unwrap();
    assert_eq!(examples.len(), 17);
    let mut cases = BTreeSet::new();
    for example in examples {
        let event = date(example["eventDate"].as_str().unwrap());
        let now = utc(example["nowUtc"].as_str().unwrap());
        assert!(cases.insert((event, now)), "duplicate {example}");
        assert_eq!(event.year(), 2026, "not a general DST/calendar algorithm");
        // Independent local-midnight derivation from the retained Swedish standard/
        // summer-time passages and EU 2026 schedule: March 29 changes AFTER its
        // midnight, as does October 25. Thus summer MIDNIGHTS are Mar 30–Oct 25.
        // Do not select the candidate day's offset from `now` (which may be 2027).
        let offset = if (date("2026-03-30")..=date("2026-10-25")).contains(&event) {
            7200
        } else {
            3600
        };
        let start = FixedOffset::east_opt(offset)
            .unwrap()
            .from_local_datetime(&event.and_hms_opt(0, 0, 0).unwrap())
            .single()
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(example["isFuture"], json!(start > now), "{example}");
    }
}
