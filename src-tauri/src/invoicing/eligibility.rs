//! Invoice-owned recognition policy; see invoice-lifecycle and tax-rules specs.
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use specta::Type;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::{error::AppError, ledger::VatBucket, rules::RuleVersionSummary};

pub const VALIDATION_ID: &str = "invoice-recognition-se-service25-2026-exact-v1";
pub const RULE_VERSION_ID: &str = "rv-2026-domestic-service25-exact-v1";
pub const POLICY_FAMILY: &str = "invoice_recognition";
pub const POLICY_KEY: &str = "domestic_service25_exact";
pub const EVIDENCE_MANIFEST: &str =
    include_str!("../../../fixtures/accounting-validation/2026-domestic-service25-exact.json");
pub const DEADLINE_SOURCE: &str = "https://www.skatteverket.se/foretag/moms/deklareramoms/narskajagdeklareramoms.4.6d02084411db6e252fe80008988.html";
const COVERAGE: &str = "2026 invoice-method recognition only: ordinary completed domestic SEK service, 25% exact VAT per line, no prior payment, annual VAT without EU trade. Not whole-book, return, credit, settlement or legal PDF validation.";

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvoiceEligibilityFacts {
    pub ordinary_domestic_service: Option<bool>,
    pub service_completed_on: Option<String>,
    pub prior_payment_received: Option<bool>,
    pub no_eu_trade_in_year: Option<bool>,
    pub annual_vat_registration_confirmed: Option<bool>,
    pub expected_taxable_base_within_limit: Option<bool>,
    // Confirms timely, consistent sending-date treatment; see invoice-lifecycle.
    pub ordinary_invoice_timing_confirmed: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvoiceEligibilityAssessment {
    pub validation_id: String,
    pub evidence_digest: String,
    pub review_fingerprint: String,
    pub accounting_date: String,
    pub completed_on: String,
    pub deadline_regime: String,
    pub period_key: String,
    pub due_on: String,
    pub coverage_summary: String,
    pub rule_version_id: String,
    pub accounting_year: i32,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceIssueValidationGetInput {
    pub invoice_id: String,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceIssueValidation {
    pub invoice_id: String,
    pub facts: InvoiceEligibilityFacts,
    pub assessment: InvoiceEligibilityAssessment,
    pub recorded_at: String,
}

// The persisted typed policy is owned by tax-rules.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CasePolicy {
    validation_id: String,
    evidence_digest: String,
    accounting_year: i32,
    accounting_method: String,
    currency: String,
    vat_rate_bp: i64,
    revenue_account: String,
    receivable_account: String,
    output_vat_account: String,
    annual_taxable_base_limit_minor: i64,
    rounding_coverage: String,
    reporting_period: String,
    deadline_regime: String,
    period_key: String,
    due_on: String,
}

fn reviewed_policy() -> CasePolicy {
    CasePolicy {
        validation_id: VALIDATION_ID.into(),
        evidence_digest: evidence_digest(),
        accounting_year: 2026,
        accounting_method: "invoice_method".into(),
        currency: "SEK".into(),
        vat_rate_bp: 2500,
        revenue_account: "3041".into(),
        receivable_account: "1510".into(),
        output_vat_account: "2611".into(),
        annual_taxable_base_limit_minor: 100_000_000,
        rounding_coverage: "exact_vat_minor_units_only".into(),
        reporting_period: "yearly".into(),
        deadline_regime: "annual_may_12".into(),
        period_key: "2026".into(),
        due_on: "2027-05-12".into(),
    }
}

fn blocked(message: impl Into<String>) -> AppError {
    AppError::validation(message, "eligibility")
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn evidence_digest() -> String {
    digest(EVIDENCE_MANIFEST.as_bytes())
}

fn encode<T: Serialize>(value: &T) -> Result<String, AppError> {
    serde_json::to_string(value).map_err(|_| blocked("Eligibility context cannot be encoded"))
}

fn parse<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, AppError> {
    serde_json::from_str(raw)
        .map_err(|_| blocked("Eligibility evidence or persisted context is invalid"))
}

fn exact_amounts(quantity: i64, unit: i64) -> Result<(i64, i64, i64), AppError> {
    if quantity <= 0 || unit <= 0 {
        return Err(blocked("Positive service line amounts are required"));
    }
    let net = quantity
        .checked_mul(unit)
        .ok_or_else(|| blocked("Amount is out of range"))?;
    if net % 4 != 0 {
        return Err(blocked("Fractional-öre VAT on any line is outside reviewed coverage; no rounding policy is validated"));
    }
    let vat = net / 4;
    let gross = net
        .checked_add(vat)
        .ok_or_else(|| blocked("Amount is out of range"))?;
    Ok((net, vat, gross))
}

fn checked_totals(
    lines: impl IntoIterator<Item = (i64, i64)>,
) -> Result<(i64, i64, i64), AppError> {
    let (mut net, mut vat) = (0i64, 0i64);
    for (line_net, line_vat) in lines {
        net = net
            .checked_add(line_net)
            .ok_or_else(|| blocked("Amount is out of range"))?;
        vat = vat
            .checked_add(line_vat)
            .ok_or_else(|| blocked("Amount is out of range"))?;
    }
    let gross = net
        .checked_add(vat)
        .ok_or_else(|| blocked("Amount is out of range"))?;
    Ok((net, vat, gross))
}

fn covered_dates(issue: &str, completion: &str) -> Result<(), AppError> {
    let date = |raw: &str| {
        NaiveDate::parse_from_str(raw, "%Y-%m-%d")
            .ok()
            .filter(|date| date.format("%Y-%m-%d").to_string() == raw)
            .ok_or_else(|| {
                blocked("Explicit valid issue and service completion dates are required")
            })
    };
    let issue = date(issue)?;
    let completion = date(completion)?;
    if issue.year() != 2026 || completion.year() != 2026 || completion > issue {
        return Err(blocked("Only services completed in 2026 no later than the 2026 sending/invoice date are covered"));
    }
    Ok(())
}

fn validate_facts<'a>(
    facts: Option<&'a InvoiceEligibilityFacts>,
    issue: &str,
) -> Result<&'a InvoiceEligibilityFacts, AppError> {
    let facts = facts
        .ok_or_else(|| blocked("Explicit case eligibility facts are required for new issuance"))?;
    if facts.ordinary_domestic_service != Some(true)
        || facts.prior_payment_received != Some(false)
        || facts.no_eu_trade_in_year != Some(true)
        || facts.annual_vat_registration_confirmed != Some(true)
        || facts.expected_taxable_base_within_limit != Some(true)
        || facts.ordinary_invoice_timing_confirmed != Some(true)
    {
        return Err(blocked("Confirm ordinary domestic service without special VAT treatment or previous payment, annual VAT/no EU trade, expected taxable base within SEK 1 million, and timely consistent invoice-date treatment with the selected date being the sending date"));
    }
    covered_dates(
        issue,
        facts
            .service_completed_on
            .as_deref()
            .ok_or_else(|| blocked("Service completion date is required"))?,
    )?;
    Ok(facts)
}

pub(crate) struct Evaluation {
    pub assessment: InvoiceEligibilityAssessment,
    pub rule: RuleVersionSummary,
    pub fiscal_year_id: String,
    pub current_turnover_minor: i64,
    pub net: i64,
    pub vat: i64,
    context: Value,
}

async fn rule_value(
    tx: &mut Transaction<'_, Sqlite>,
    family: &str,
    key: &str,
) -> Result<Value, AppError> {
    let rows: Vec<String> = sqlx::query_scalar(
        "SELECT value_json FROM tax_rules WHERE rule_version_id = ?1 AND family = ?2 AND key = ?3",
    )
    .bind(RULE_VERSION_ID)
    .bind(family)
    .bind(key)
    .fetch_all(&mut **tx)
    .await?;
    if rows.len() != 1 {
        return Err(blocked("Applicable reviewed rule is missing or duplicated"));
    }
    parse(&rows[0])
}

/// Evaluate a new recognition case against this transaction's actual inputs.
pub(crate) async fn evaluate_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    invoice_id: &str,
    issue: &str,
    facts: Option<&InvoiceEligibilityFacts>,
) -> Result<Evaluation, AppError> {
    // Preserve accountingMethod as the first guard even when facts are absent.
    crate::profiles::ensure_invoice_timing_supported(&mut **tx, workspace_id).await?;
    let facts = validate_facts(facts, issue)?;
    let invoice = sqlx::query("SELECT counterparty_id, due_date, status, invoice_kind, voucher_id, total_ex_vat_minor, total_vat_minor, total_inc_vat_minor FROM invoices WHERE workspace_id = ?1 AND id = ?2")
        .bind(workspace_id).bind(invoice_id).fetch_optional(&mut **tx).await?
        .ok_or_else(|| AppError::validation("Invoice not found", "invoiceId"))?;
    if invoice.try_get::<String, _>("status")? != "draft"
        || invoice.try_get::<String, _>("invoice_kind")? != "standard"
        || invoice
            .try_get::<Option<String>, _>("voucher_id")?
            .is_some()
    {
        return Err(blocked("Only a new standard unposted draft is eligible"));
    }
    let posted: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM vouchers WHERE workspace_id = ?1 AND source_id = ?2 AND source_type = 'invoice'")
        .bind(workspace_id).bind(invoice_id).fetch_one(&mut **tx).await?;
    if posted != 0 {
        return Err(blocked("An invoice posting already exists"));
    }
    let tax_status = super::validate_issue_tax_status_tx(tx, workspace_id).await?;
    let profiles = sqlx::query("SELECT vat_status, reporting_period, accounting_method, voluntary_registration_date, vat_filing_deadline_regime FROM vat_profiles WHERE workspace_id = ?1")
        .bind(workspace_id).fetch_all(&mut **tx).await?;
    if profiles.len() != 1 {
        return Err(blocked("A single applicable VAT profile is required"));
    }
    let profile = &profiles[0];
    let vat_status: String = profile.try_get("vat_status")?;
    let voluntary_date: Option<String> = profile.try_get("voluntary_registration_date")?;
    let completion = facts.service_completed_on.as_deref().unwrap();
    let completion_date = NaiveDate::parse_from_str(completion, "%Y-%m-%d")
        .map_err(|_| blocked("A valid service completion date is required"))?;
    let registration_date = voluntary_date
        .as_deref()
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok());
    if vat_status != "registered"
        && !(vat_status == "voluntary_registered"
            && registration_date.is_some_and(|date| date <= completion_date))
    {
        return Err(blocked(
            "VAT registration must apply when the service was supplied",
        ));
    }
    if profile.try_get::<String, _>("reporting_period")? != "yearly"
        || profile
            .try_get::<Option<String>, _>("vat_filing_deadline_regime")?
            .as_deref()
            != Some("annual_may_12")
    {
        return Err(blocked(
            "Only annual VAT without EU trade under annual_may_12 is covered",
        ));
    }
    let versions = sqlx::query("SELECT id, source_url, effective_from, effective_to FROM rule_versions WHERE tax_year = 2026 AND status = 'active'")
        .fetch_all(&mut **tx).await?;
    if versions.len() != 1 || versions[0].try_get::<String, _>("id")? != RULE_VERSION_ID {
        return Err(blocked(
            "Exactly one reviewed active rule version for 2026 is required",
        ));
    }
    let version = &versions[0];
    let effective_from: String = version.try_get("effective_from")?;
    let effective_to: Option<String> = version.try_get("effective_to")?;
    if effective_from != "2026-01-01" || effective_to.as_deref() != Some("2026-12-31") {
        return Err(blocked("Reviewed rule applicability has changed"));
    }
    let policy_value = rule_value(tx, POLICY_FAMILY, POLICY_KEY).await?;
    let policy: CasePolicy = parse(&encode(&policy_value)?)?;
    if policy != reviewed_policy() {
        return Err(blocked(
            "Persisted case policy differs from the compiled reviewed evidence",
        ));
    }
    let calendar = rule_value(tx, "vat", "filing_deadline_schedules").await?;
    if calendar["sourceUrl"] != DEADLINE_SOURCE
        || calendar["schedules"]["annual_may_12"]["2026"] != "2027-05-12"
    {
        return Err(blocked(
            "Reviewed accounting-period 2026 filing deadline is missing or altered",
        ));
    }
    let years = sqlx::query("SELECT id, starts_on, ends_on FROM fiscal_years WHERE workspace_id = ?1 AND starts_on <= ?2 AND ends_on >= ?2")
        .bind(workspace_id).bind(issue).fetch_all(&mut **tx).await?;
    if years.len() != 1
        || years[0].try_get::<String, _>("starts_on")? != "2026-01-01"
        || years[0].try_get::<String, _>("ends_on")? != "2026-12-31"
    {
        return Err(blocked(
            "A single calendar accounting year 2026 is required",
        ));
    }
    let fiscal_year_id: String = years[0].try_get("id")?;
    let rows = sqlx::query("SELECT id, line_order, description, quantity, unit_price_minor, vat_rate_bp, account_number FROM invoice_lines WHERE invoice_id = ?1 ORDER BY line_order, id")
        .bind(invoice_id).fetch_all(&mut **tx).await?;
    if rows.is_empty() {
        return Err(blocked("At least one completed service line is required"));
    }
    let mut lines = Vec::new();
    let mut amounts = Vec::new();
    for row in rows {
        if row.try_get::<i64, _>("vat_rate_bp")? != 2500
            || row.try_get::<String, _>("account_number")? != "3041"
        {
            return Err(blocked(
                "Only 25% ordinary domestic service lines on account 3041 are covered",
            ));
        }
        let quantity = row.try_get("quantity")?;
        let unit = row.try_get("unit_price_minor")?;
        let (net, vat, _) = exact_amounts(quantity, unit)?;
        amounts.push((net, vat));
        lines.push(json!({"id": row.try_get::<String,_>("id")?, "order": row.try_get::<i64,_>("line_order")?,
            "description": row.try_get::<String,_>("description")?, "quantity": quantity, "unitPriceMinor": unit,
            "vatRateBp": 2500, "accountNumber": "3041", "netMinor": net, "vatMinor": vat}));
    }
    let (net, vat, gross) = checked_totals(amounts)?;
    if invoice.try_get::<i64, _>("total_ex_vat_minor")? != net
        || invoice.try_get::<i64, _>("total_vat_minor")? != vat
        || invoice.try_get::<i64, _>("total_inc_vat_minor")? != gross
    {
        return Err(blocked(
            "Stored invoice totals do not equal checked exact line amounts",
        ));
    }
    let current_turnover_minor =
        super::net_revenue_minor_for_fiscal_year_tx(tx, workspace_id, &fiscal_year_id).await?;
    let projected = current_turnover_minor
        .checked_add(net)
        .ok_or_else(|| blocked("Annual taxable base is out of range"))?;
    if projected > policy.annual_taxable_base_limit_minor {
        return Err(blocked(
            "Current supported revenue plus invoice exceeds the annual VAT taxable-base limit",
        ));
    }
    let source_url: String = version.try_get("source_url")?;
    let context = json!({"validationId": VALIDATION_ID, "evidenceDigest": evidence_digest(),
        "workspaceId": workspace_id, "invoiceId": invoice_id, "fiscalYearId": fiscal_year_id,
        "issueDate": issue, "facts": facts, "counterpartyId": invoice.try_get::<String,_>("counterparty_id")?,
        "dueDate": invoice.try_get::<Option<String>,_>("due_date")?, "lines": lines,
        "netMinor": net, "vatMinor": vat, "grossMinor": gross, "currentTurnoverMinor": current_turnover_minor,
        "profile": {"taxStatus": tax_status, "vatStatus": vat_status, "reportingPeriod": "yearly",
            "accountingMethod": "invoice_method", "voluntaryRegistrationDate": voluntary_date, "deadlineRegime": "annual_may_12"},
        "rules": {"id": RULE_VERSION_ID, "effectiveFrom": effective_from, "effectiveTo": effective_to,
            "sourceUrl": source_url, "policy": policy_value, "calendar": calendar}});
    let assessment = assessment(issue, completion, digest(encode(&context)?.as_bytes()));
    Ok(Evaluation {
        assessment,
        context,
        fiscal_year_id,
        current_turnover_minor,
        net,
        vat,
        rule: RuleVersionSummary {
            id: RULE_VERSION_ID.into(),
            tax_year: 2026,
            source_url,
            status: "active".into(),
        },
    })
}

fn assessment(issue: &str, completion: &str, fingerprint: String) -> InvoiceEligibilityAssessment {
    InvoiceEligibilityAssessment {
        validation_id: VALIDATION_ID.into(),
        evidence_digest: evidence_digest(),
        review_fingerprint: fingerprint,
        accounting_date: issue.into(),
        completed_on: completion.into(),
        deadline_regime: "annual_may_12".into(),
        period_key: "2026".into(),
        due_on: "2027-05-12".into(),
        coverage_summary: COVERAGE.into(),
        rule_version_id: RULE_VERSION_ID.into(),
        accounting_year: 2026,
    }
}

pub(crate) async fn record_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    invoice_id: &str,
    facts: &InvoiceEligibilityFacts,
    evaluation: &Evaluation,
) -> Result<(), AppError> {
    sqlx::query("INSERT INTO invoice_issue_validations (invoice_id, workspace_id, facts_json, assessment_json, context_json) VALUES (?1, ?2, ?3, ?4, ?5)")
        .bind(invoice_id).bind(workspace_id).bind(encode(facts)?).bind(encode(&evaluation.assessment)?).bind(encode(&evaluation.context)?)
        .execute(&mut **tx).await?;
    Ok(())
}

fn decode_record(
    row: &sqlx::sqlite::SqliteRow,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<(InvoiceIssueValidation, Value), AppError> {
    let facts: InvoiceEligibilityFacts = parse(&row.try_get::<String, _>("facts_json")?)?;
    let stored: InvoiceEligibilityAssessment =
        parse(&row.try_get::<String, _>("assessment_json")?)?;
    let context: Value = parse(&row.try_get::<String, _>("context_json")?)?;
    validate_facts(Some(&facts), &stored.accounting_date)?;
    let fingerprint = digest(encode(&context)?.as_bytes());
    if stored
        != assessment(
            &stored.accounting_date,
            facts.service_completed_on.as_deref().unwrap(),
            fingerprint,
        )
        || context["workspaceId"] != workspace_id
        || context["invoiceId"] != invoice_id
        || context["facts"]
            != serde_json::to_value(&facts).map_err(|_| blocked("Invalid retained facts"))?
        || context["issueDate"] != stored.accounting_date
        || context["evidenceDigest"] != stored.evidence_digest
    {
        return Err(blocked("Retained eligibility provenance is inconsistent"));
    }
    Ok((
        InvoiceIssueValidation {
            invoice_id: invoice_id.into(),
            facts,
            assessment: stored,
            recorded_at: row.try_get("recorded_at")?,
        },
        context,
    ))
}

/// Revalidate immutable pending provenance against this posting transaction.
pub(crate) async fn authorize_post_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    fiscal_year_id: &str,
    invoice_id: &str,
    issue: &str,
    buckets: &[VatBucket],
) -> Result<(), AppError> {
    let row = sqlx::query("SELECT facts_json, assessment_json, context_json, recorded_at FROM invoice_issue_validations WHERE workspace_id = ?1 AND invoice_id = ?2")
        .bind(workspace_id).bind(invoice_id).fetch_optional(&mut **tx).await?
        .ok_or_else(|| blocked("Invoice posting requires pending case validation from issuance"))?;
    let (record, _) = decode_record(&row, workspace_id, invoice_id)?;
    let current = evaluate_tx(tx, workspace_id, invoice_id, issue, Some(&record.facts)).await?;
    if current.assessment != record.assessment
        || current.fiscal_year_id != fiscal_year_id
        || buckets.len() != 1
        || buckets[0].vat_code != "VAT25"
        || buckets[0].ex_vat_minor != current.net
        || buckets[0].vat_minor != current.vat
    {
        return Err(blocked("Invoice posting context, accounting date/year or VAT buckets differ from validated pending issuance"));
    }
    Ok(())
}

pub async fn invoice_issue_validation_get(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &InvoiceIssueValidationGetInput,
) -> Result<Option<InvoiceIssueValidation>, AppError> {
    let row = sqlx::query("SELECT v.facts_json, v.assessment_json, v.context_json, v.recorded_at, i.issue_date FROM invoice_issue_validations v JOIN invoices i ON i.id = v.invoice_id AND i.workspace_id = v.workspace_id WHERE v.workspace_id = ?1 AND v.invoice_id = ?2 AND i.status != 'draft'")
        .bind(workspace_id).bind(&input.invoice_id).fetch_optional(pool).await?;
    row.map(|row| {
        let (record, _) = decode_record(&row, workspace_id, &input.invoice_id)?;
        if row.try_get::<Option<String>, _>("issue_date")?.as_deref()
            != Some(&record.assessment.accounting_date)
        {
            return Err(blocked(
                "Retained eligibility accounting date differs from issued invoice",
            ));
        }
        Ok(record)
    })
    .transpose()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_amounts_and_checked_boundaries() {
        assert_eq!(
            exact_amounts(1, 1_000_000).unwrap(),
            (1_000_000, 250_000, 1_250_000)
        );
        for (quantity, unit) in [(1, 4), (2, 2), (4, 1)] {
            assert_eq!(exact_amounts(quantity, unit).unwrap(), (4, 1, 5));
        }
        for unit in [1, 2, 3] {
            assert!(exact_amounts(1, unit).is_err());
        }
        assert!(exact_amounts(2, i64::MAX).is_err());
        assert!(exact_amounts(1, i64::MAX - 3).is_err());
        assert!(exact_amounts(0, 4).is_err());
        assert!(exact_amounts(-1, 4).is_err());
        assert!(exact_amounts(1, -4).is_err());
        assert!(checked_totals([(i64::MAX, 0), (4, 1)]).is_err());
        assert_eq!(checked_totals([(8, 2), (12, 3)]).unwrap(), (20, 5, 25));
    }

    #[test]
    fn evidence_case_dates_and_no_rounding_permission() {
        assert!(covered_dates("2026-12-31", "2026-12-01").is_ok());
        assert!(covered_dates("2026-12-31", "2026-12-31").is_ok());
        for (issue, completion) in [
            ("2026-01-01", "2026-01-02"),
            ("2026-01-01", "2025-12-31"),
            ("2027-01-01", "2026-12-31"),
            ("2026-02-30", "2026-02-01"),
        ] {
            assert!(covered_dates(issue, completion).is_err());
        }
        let manifest: serde_json::Value = serde_json::from_str(EVIDENCE_MANIFEST).unwrap();
        assert_eq!(
            manifest["scope"]["roundingCoverage"],
            "exact_vat_minor_units_only"
        );
        assert_eq!(manifest["calendar"]["dueOn"], "2027-05-12");
        assert_eq!(manifest["validationId"], VALIDATION_ID);
        assert_eq!(manifest["ruleVersionId"], RULE_VERSION_ID);
        assert_eq!(evidence_digest().len(), 64);
    }
}
