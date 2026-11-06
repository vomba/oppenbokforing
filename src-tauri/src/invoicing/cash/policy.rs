//! Source-backed cash predicates; never delegates permission to invoice timing.
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde_json::{json, Value};
use sqlx::{Row, Sqlite, Transaction};

use super::*;
use crate::invoicing::{eligibility, validation_registry::CASH_V1, InvoiceIssueReviewSnapshot};

pub(super) fn date(raw: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .filter(|d| d.year() == 2026 && d.format("%Y-%m-%d").to_string() == raw)
        .ok_or_else(|| blocked("An explicit ISO date in 2026 is required"))
}

// Only this reviewed finite year's data is used. No general DST algorithm.
pub(super) fn ensure_not_future(raw: &str, now: DateTime<Utc>) -> Result<(), AppError> {
    let day = date(raw)?
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| blocked("Invalid civil day"))?
        .and_utc();
    let manifest: Value = decode(CASH_V1.manifest)?;
    let clock = &manifest["clockPolicy"];
    let begin = clock["summerBeginsUtc"]
        .as_str()
        .ok_or_else(|| blocked("Missing clock evidence"))?
        .parse::<DateTime<Utc>>()
        .map_err(|_| blocked("Invalid clock evidence"))?;
    let end = clock["summerEndsUtc"]
        .as_str()
        .ok_or_else(|| blocked("Missing clock evidence"))?
        .parse::<DateTime<Utc>>()
        .map_err(|_| blocked("Invalid clock evidence"))?;
    // At civil midnight on transition days the old offset still applies.
    let standard = clock["standardOffsetSeconds"]
        .as_i64()
        .ok_or_else(|| blocked("Missing offset"))?;
    let summer = clock["summerOffsetSeconds"]
        .as_i64()
        .ok_or_else(|| blocked("Missing offset"))?;
    let standard_candidate = day - Duration::seconds(standard);
    let offset = if standard_candidate >= begin && standard_candidate < end {
        summer
    } else {
        standard
    };
    if day - Duration::seconds(offset) > now {
        return Err(blocked("A future Swedish civil issue date is not covered"));
    }
    Ok(())
}

pub(super) fn reviewed_policy() -> Result<Value, AppError> {
    let manifest: Value = decode(CASH_V1.manifest)?;
    Ok(
        json!({"validationId":CASH_V1.id,"evidenceDigest":CASH_V1.digest(),
        "scope":manifest["scope"],"calendar":manifest["calendar"],
        "clockPolicy":manifest["clockPolicy"],"recognition":manifest["recognition"]}),
    )
}

pub(super) fn validate_facts(
    facts: &CashIssueFacts,
    issue: &str,
    origin: &CashBusinessOriginFacts,
) -> Result<(), AppError> {
    if facts.ordinary_domestic_service != Some(true)
        || facts.prior_payment_received != Some(false)
        || facts.no_eu_trade_in_year != Some(true)
        || facts.annual_vat_registration_confirmed != Some(true)
        || facts.expected_taxable_base_within_limit != Some(true)
        || facts.expected_net_turnover_within_cash_limit != Some(true)
        || facts.ordinary_timely_sending_confirmed != Some(true)
        || facts.registered_bokslutsmetoden_confirmed != Some(true)
        || facts.cash_treatment_consistent_confirmed != Some(true)
    {
        return Err(blocked("Explicit ordinary service, no prior payment, annual/no-EU VAT, separate turnover/base limits, timely sending and registered consistent cash treatment are required"));
    }
    let completion = date(
        facts
            .service_completed_on
            .as_deref()
            .ok_or_else(|| blocked("Completion date required"))?,
    )?;
    let issue = date(issue)?;
    if completion > issue {
        return Err(blocked("Service must be completed by issue date"));
    }
    if origin.new_business_started_in_2026 != Some(true)
        || origin.zero_opening_balances_confirmed != Some(true)
        || origin.no_opening_unpaid_invoices_or_bills_confirmed != Some(true)
        || origin.no_prior_unrecorded_activity_confirmed != Some(true)
        || origin.outside_workspace_history_reviewed != Some(true)
        || origin.first_financial_year_ends_on.as_deref() != Some("2026-12-31")
    {
        return Err(blocked("Explicit new 2026 business and December-ending first year, zero opening, no unpaid or prior activity and outside-workspace review are required"));
    }
    let start = date(
        origin
            .business_started_on
            .as_deref()
            .ok_or_else(|| blocked("Actual business start required"))?,
    )?;
    if start > completion {
        return Err(blocked("Business start cannot follow completion"));
    }
    if start.to_string() != "2026-01-01"
        && facts.short_year_turnover_adjustment_reviewed != Some(true)
    {
        return Err(blocked("Short first years require explicit BFN 1.7 twelve-month-adjusted normal turnover review"));
    }
    if facts.short_year_turnover_adjustment_reviewed == Some(false) {
        return Err(blocked("Contradictory turnover adjustment review"));
    }
    Ok(())
}

pub(super) struct Evaluation {
    pub preflight: CashIssuePreflight,
    pub origin: CashBusinessOrigin,
    pub new_origin: bool,
    pub fiscal_year_id: String,
    pub context: Value,
    pub tax_status: String,
    pub vat_status: String,
}

pub(super) async fn history_supported(
    tx: &mut Transaction<'_, Sqlite>,
    workspace: &str,
    first: bool,
) -> Result<(), AppError> {
    let incompatible: bool = if first {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM vouchers WHERE workspace_id=?1 AND status!='draft') OR EXISTS(SELECT 1 FROM invoices WHERE workspace_id=?1 AND status!='draft') OR EXISTS(SELECT 1 FROM reconciliation_matches WHERE workspace_id=?1)")
            .bind(workspace).fetch_one(&mut **tx).await?
    } else {
        // Ordinary expenses can consume imported bank staging too. The label alone
        // is not authority: require the scoped posting, its original expense-job
        // staging link, actual outflow/date and compatible balanced expense lines.
        // Cash receipt proof remains a separate, unchanged continuation route.
        sqlx::query_scalar(r#"
            SELECT EXISTS(SELECT 1 FROM invoices i WHERE i.workspace_id=?1 AND i.status!='draft' AND (i.status!='issued' OR i.invoice_kind!='standard' OR i.voucher_id IS NOT NULL OR NOT EXISTS(SELECT 1 FROM cash_invoice_issues c WHERE c.workspace_id=i.workspace_id AND c.invoice_id=i.id)))
            OR EXISTS(
                SELECT 1 FROM reconciliation_matches m WHERE m.workspace_id=?1
                AND NOT EXISTS(SELECT 1 FROM cash_receipt_recognitions r WHERE r.workspace_id=m.workspace_id AND r.match_id=m.id)
                AND NOT EXISTS(
                    SELECT 1 FROM vouchers v
                    JOIN staged_transactions s ON s.id=m.staged_transaction_id AND s.workspace_id=m.workspace_id
                    JOIN fiscal_years y ON y.id=v.fiscal_year_id AND y.workspace_id=v.workspace_id
                    JOIN cash_business_origins o ON o.workspace_id=v.workspace_id AND o.fiscal_year_id=y.id
                    WHERE v.id=m.voucher_id AND v.workspace_id=m.workspace_id
                    AND m.match_kind='expense' AND m.invoice_id IS NULL
                    AND v.status='posted' AND v.source_type='expense' AND v.source_id IS NULL
                    AND s.status='matched' AND s.amount_minor<0
                    AND v.accounting_date=s.transaction_date
                    AND v.accounting_date BETWEEN y.starts_on AND y.ends_on
                    AND v.accounting_date BETWEEN json_extract(o.facts_json,'$.businessStartedOn') AND '2026-12-31'
                    AND EXISTS(
                        SELECT 1 FROM local_jobs job WHERE job.workspace_id=m.workspace_id
                        AND job.job_type='expense_post' AND job.status='succeeded'
                        AND json_extract(job.payload_json,'$.stagedTransactionId')=s.id
                        AND json_extract(job.payload_json,'$.result.voucherId')=v.id
                    )
                    AND EXISTS(
                        SELECT 1 FROM journal_lines j JOIN accounts a ON a.id=j.account_id
                        WHERE j.voucher_id=v.id AND a.workspace_id=v.workspace_id
                        AND a.account_type='expense' AND a.number BETWEEN '4000' AND '8999'
                        AND j.debit_minor>0 AND j.credit_minor=0
                    )
                    AND NOT EXISTS(
                        SELECT 1 FROM journal_lines j JOIN accounts a ON a.id=j.account_id
                        WHERE j.voucher_id=v.id AND (a.workspace_id!=v.workspace_id OR NOT (
                            (a.account_type='expense' AND a.number BETWEEN '4000' AND '8999' AND j.debit_minor>0 AND j.credit_minor=0)
                            OR (a.number='2641' AND j.debit_minor>0 AND j.credit_minor=0)
                            OR (a.number='1930' AND j.credit_minor>0 AND j.debit_minor=0)
                        ))
                    )
                    AND (SELECT SUM(j.credit_minor) FROM journal_lines j WHERE j.voucher_id=v.id)=-s.amount_minor
                    AND (SELECT SUM(j.debit_minor) FROM journal_lines j WHERE j.voucher_id=v.id)=-s.amount_minor
                )
            )
            OR EXISTS(SELECT 1 FROM vouchers v WHERE v.workspace_id=?1 AND v.status!='draft' AND NOT EXISTS(SELECT 1 FROM cash_receipt_recognitions r WHERE r.workspace_id=v.workspace_id AND r.voucher_id=v.id) AND (v.status!='posted' OR v.source_type!='expense' OR EXISTS(SELECT 1 FROM journal_lines j JOIN accounts a ON a.id=j.account_id WHERE j.voucher_id=v.id AND (a.workspace_id!=v.workspace_id OR a.number BETWEEN '3000' AND '3999' OR a.number IN ('1510','2611')))))
        "#)
            .bind(workspace).fetch_one(&mut **tx).await?
    };
    if incompatible {
        return Err(blocked("Actual posted, issued or payment history contradicts the reviewed cash origin or continuation"));
    }
    Ok(())
}

pub(super) async fn evaluate_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace: &str,
    input: &CashIssuePreflightInput,
) -> Result<Evaluation, AppError> {
    evaluate_stage_tx(tx, workspace, input, None).await
}

// Receipt continuation reuses the actual policy predicates, never draft eligibility.
// Its accounting/lock date is the receipt date and its issue is already committed.
pub(super) async fn evaluate_stage_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace: &str,
    input: &CashIssuePreflightInput,
    receipt_date: Option<&str>,
) -> Result<Evaluation, AppError> {
    let accounting_date = receipt_date.unwrap_or(&input.issue_date);
    ensure_not_future(accounting_date, Utc::now())?;
    let retained = origin_tx(tx, workspace).await?;
    let new_origin = retained.is_none();
    let origin = match retained {
        Some(origin) => {
            if let Some(review) = &input.origin_review {
                if value(review)? != value(&origin.facts)? {
                    return Err(blocked("Origin review differs from immutable origin"));
                }
            }
            origin
        }
        None => {
            let facts = input.origin_review.clone().ok_or_else(|| {
                blocked("First cash issue requires explicit business origin review")
            })?;
            CashBusinessOrigin {
                id: origin_id(workspace, &input.invoice_id, &facts)?,
                facts,
                first_invoice_id: input.invoice_id.clone(),
                recorded_at: String::new(),
            }
        }
    };
    validate_facts(&input.facts, &input.issue_date, &origin.facts)?;
    history_supported(tx, workspace, new_origin).await?;
    let businesses = sqlx::query("SELECT business_name,owner_name,residency_country,sni_code FROM sole_trader_profiles WHERE workspace_id=?1").bind(workspace).fetch_all(&mut **tx).await?;
    if businesses.len() != 1 || businesses[0].try_get::<String, _>("residency_country")? != "SE" {
        return Err(blocked(
            "One explicit Swedish sole-trader profile is required",
        ));
    }
    let business = &businesses[0];
    let taxes=sqlx::query("SELECT tax_status,expected_business_profit_minor,expected_salary_income_minor,active_rule_year FROM tax_profiles WHERE workspace_id=?1").bind(workspace).fetch_all(&mut **tx).await?;
    if taxes.len() != 1 {
        return Err(blocked("One tax profile is required"));
    }
    let tax = &taxes[0];
    let tax_status = super::super::validate_issue_tax_status_tx(tx, workspace).await?;
    if tax_status == "fa_skatt" && tax.try_get::<i64, _>("expected_salary_income_minor")? <= 0 {
        return Err(blocked(
            "FA-skatt requires the validated positive salary posture",
        ));
    }
    let profiles=sqlx::query("SELECT vat_status,reporting_period,accounting_method,voluntary_registration_date,vat_filing_deadline_regime FROM vat_profiles WHERE workspace_id=?1").bind(workspace).fetch_all(&mut **tx).await?;
    if profiles.len() != 1 {
        return Err(blocked("One VAT profile is required"));
    }
    let profile = &profiles[0];
    let vat_status: String = profile.try_get("vat_status")?;
    let voluntary: Option<String> = profile.try_get("voluntary_registration_date")?;
    if profile.try_get::<String, _>("accounting_method")? != "cash_method"
        || profile.try_get::<String, _>("reporting_period")? != "yearly"
        || profile
            .try_get::<Option<String>, _>("vat_filing_deadline_regime")?
            .as_deref()
            != Some("annual_may_12")
    {
        return Err(blocked(
            "Registered annual/no-EU cash method under annual_may_12 is required",
        ));
    }
    let completion = input.facts.service_completed_on.as_deref().unwrap();
    let completion_date = date(completion)?;
    if vat_status != "registered"
        && !(vat_status == "voluntary_registered"
            && voluntary
                .as_deref()
                .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                .is_some_and(|d| d <= completion_date))
    {
        return Err(blocked("VAT registration must be effective by completion"));
    }
    let versions=sqlx::query("SELECT id,effective_from,effective_to,source_url FROM rule_versions WHERE tax_year=2026 AND status='active'").fetch_all(&mut **tx).await?;
    if versions.len() != 1
        || versions[0].try_get::<String, _>("id")? != CASH_V1.rule_id
        || versions[0].try_get::<String, _>("effective_from")? != "2026-01-01"
        || versions[0]
            .try_get::<Option<String>, _>("effective_to")?
            .as_deref()
            != Some("2026-12-31")
    {
        return Err(blocked(
            "Exactly the reviewed active 2026 rule version is required",
        ));
    }
    let policy_raw:Option<String>=sqlx::query_scalar("SELECT value_json FROM tax_rules WHERE rule_version_id=?1 AND family='cash_recognition' AND key='domestic_service25_pdf_full'").bind(CASH_V1.rule_id).fetch_optional(&mut **tx).await?;
    let policy: Value = decode(&policy_raw.ok_or_else(|| blocked("Cash rule policy missing"))?)?;
    if policy != reviewed_policy()? {
        return Err(blocked(
            "Cash policy differs from immutable compiled evidence",
        ));
    }
    let calendar_raw:Option<String>=sqlx::query_scalar("SELECT value_json FROM tax_rules WHERE rule_version_id=?1 AND family='vat' AND key='filing_deadline_schedules'").bind(CASH_V1.rule_id).fetch_optional(&mut **tx).await?;
    let calendar: Value = decode(&calendar_raw.ok_or_else(|| blocked("Annual calendar missing"))?)?;
    if calendar["sourceUrl"] != eligibility::DEADLINE_SOURCE
        || calendar["schedules"]["annual_may_12"]["2026"] != "2027-05-12"
    {
        return Err(blocked("Reviewed annual calendar changed"));
    }
    let years=sqlx::query("SELECT id,starts_on,ends_on,status FROM fiscal_years WHERE workspace_id=?1 AND starts_on<=?2 AND ends_on>=?2").bind(workspace).bind(accounting_date).fetch_all(&mut **tx).await?;
    if years.len() != 1 || years[0].try_get::<String, _>("ends_on")? != "2026-12-31" {
        return Err(blocked(
            "One December-ending first accounting year is required",
        ));
    }
    let year = &years[0];
    let fiscal_year_id: String = year.try_get("id")?;
    let start = origin.facts.business_started_on.as_deref().unwrap();
    let stored_start: String = year.try_get("starts_on")?;
    if (new_origin && stored_start != "2026-01-01" && stored_start != start)
        || (!new_origin && stored_start != start)
    {
        return Err(blocked(
            "Stored accounting year conflicts with actual first-year start",
        ));
    }
    crate::workspace::ensure_fiscal_year_open_tx(&mut **tx, &fiscal_year_id).await?;
    crate::vat::ensure_fiscal_period_open_tx(tx, workspace, accounting_date).await?;
    if new_origin {
        let locked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM fiscal_periods WHERE workspace_id=?1 AND fiscal_year_id=?2 AND status='locked')").bind(workspace).bind(&fiscal_year_id).fetch_one(&mut **tx).await?;
        if locked {
            return Err(blocked("Cannot configure a locked first year"));
        }
    }
    let invoice=sqlx::query("SELECT counterparty_id,due_date,status,invoice_kind,voucher_id,total_ex_vat_minor,total_vat_minor,total_inc_vat_minor FROM invoices WHERE workspace_id=?1 AND id=?2").bind(workspace).bind(&input.invoice_id).fetch_optional(&mut **tx).await?.ok_or_else(|| blocked("Invoice not found"))?;
    if invoice.try_get::<String, _>("status")?
        != if receipt_date.is_some() {
            "issued"
        } else {
            "draft"
        }
        || invoice.try_get::<String, _>("invoice_kind")? != "standard"
        || invoice
            .try_get::<Option<String>, _>("voucher_id")?
            .is_some()
    {
        return Err(blocked("Only a new standard unposted draft is eligible"));
    }
    let posting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM vouchers WHERE workspace_id=?1 AND source_id=?2 AND source_type='invoice')").bind(workspace).bind(&input.invoice_id).fetch_one(&mut **tx).await?;
    if posting {
        return Err(blocked("Invoice posting already exists"));
    }
    let rows=sqlx::query("SELECT id,line_order,description,quantity,unit_price_minor,vat_rate_bp,account_number FROM invoice_lines WHERE invoice_id=?1 ORDER BY line_order,id").bind(&input.invoice_id).fetch_all(&mut **tx).await?;
    if rows.is_empty() {
        return Err(blocked("Completed service lines required"));
    }
    let mut lines = Vec::new();
    for row in rows {
        if row.try_get::<i64, _>("vat_rate_bp")? != 2500
            || row.try_get::<String, _>("account_number")? != "3041"
        {
            return Err(blocked("Only exact 25% service lines on 3041 are covered"));
        }
        let quantity = row.try_get("quantity")?;
        let unit = row.try_get("unit_price_minor")?;
        let (net, vat, _) = eligibility::exact_amounts(quantity, unit)?;
        lines.push(super::super::InvoiceLine {
            id: row.try_get("id")?,
            line_order: row.try_get("line_order")?,
            description: row.try_get("description")?,
            quantity,
            unit_price_minor: unit,
            vat_rate_bp: 2500,
            account_number: "3041".into(),
            line_ex_vat_minor: net,
            line_vat_minor: vat,
        });
    }
    let (net, vat, gross) = eligibility::checked_totals(
        lines
            .iter()
            .map(|l| (l.line_ex_vat_minor, l.line_vat_minor)),
    )?;
    if invoice.try_get::<i64, _>("total_ex_vat_minor")? != net
        || invoice.try_get::<i64, _>("total_vat_minor")? != vat
        || invoice.try_get::<i64, _>("total_inc_vat_minor")? != gross
    {
        return Err(blocked(
            "Stored invoice totals differ from checked exact lines",
        ));
    }
    // A paid receipt replaces its commitment, not an additional taxable sale.
    let commitments:Vec<i64>=sqlx::query_scalar("SELECT net_minor FROM cash_invoice_issues WHERE workspace_id=?1 AND NOT EXISTS(SELECT 1 FROM cash_receipt_recognitions r WHERE r.workspace_id=cash_invoice_issues.workspace_id AND r.invoice_id=cash_invoice_issues.invoice_id)").bind(workspace).fetch_all(&mut **tx).await?;
    let mut current =
        super::super::net_revenue_minor_for_fiscal_year_tx(tx, workspace, &fiscal_year_id).await?;
    for amount in commitments {
        current = current
            .checked_add(amount)
            .ok_or_else(|| blocked("Annual base overflow"))?;
    }
    let projected = current
        .checked_add(if receipt_date.is_some() { 0 } else { net })
        .ok_or_else(|| blocked("Annual base overflow"))?;
    if current < 0 || projected > 100_000_000 {
        return Err(blocked(
            "Known annual base including unpaid commitments exceeds SEK 1 million",
        ));
    }
    let customer: String = invoice.try_get("counterparty_id")?;
    let customer_name: String = sqlx::query_scalar(
        "SELECT name FROM counterparties WHERE workspace_id=?1 AND id=?2 AND kind='customer'",
    )
    .bind(workspace)
    .bind(&customer)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| blocked("Reviewed customer must belong to workspace"))?;
    let reviewed = InvoiceIssueReviewSnapshot {
        invoice_id: input.invoice_id.clone(),
        counterparty_id: customer,
        counterparty_name: customer_name,
        due_date: invoice.try_get("due_date")?,
        total_ex_vat_minor: net,
        total_vat_minor: vat,
        total_inc_vat_minor: gross,
        lines,
    };
    let context = json!({"validationId":CASH_V1.id,"evidenceDigest":CASH_V1.digest(),"workspaceId":workspace,"invoiceId":input.invoice_id,
        "issueDate":input.issue_date,"facts":input.facts,"originId":origin.id,"originFacts":origin.facts,"firstInvoiceId":origin.first_invoice_id,
        "reviewedInvoice":reviewed,"fiscalYearId":fiscal_year_id,"storedYearStart":stored_start,"yearEnd":"2026-12-31","currentTaxableBaseMinor":current,
        "business":{"businessName":business.try_get::<String,_>("business_name")?,"ownerName":business.try_get::<String,_>("owner_name")?,"residencyCountry":"SE","sniCode":business.try_get::<Option<String>,_>("sni_code")?},
        "tax":{"taxStatus":tax_status,"expectedProfit":tax.try_get::<i64,_>("expected_business_profit_minor")?,"expectedSalary":tax.try_get::<i64,_>("expected_salary_income_minor")?,"activeRuleYear":tax.try_get::<i32,_>("active_rule_year")?},
        "vat":{"vatStatus":vat_status,"accountingMethod":"cash_method","reportingPeriod":"yearly","voluntaryRegistrationDate":voluntary,"deadlineRegime":"annual_may_12"},
        "rules":{"id":CASH_V1.rule_id,"sourceUrl":versions[0].try_get::<String,_>("source_url")?,"effectiveFrom":"2026-01-01","effectiveTo":"2026-12-31","policy":policy,"calendar":calendar}});
    let assessment = assessment(
        &input.issue_date,
        completion,
        &origin.id,
        hash(encode(&context)?.as_bytes()),
    );
    Ok(Evaluation {
        preflight: CashIssuePreflight {
            invoice_id: input.invoice_id.clone(),
            issue_date: input.issue_date.clone(),
            assessment,
            reviewed_invoice: reviewed,
            current_taxable_base_minor: current,
            projected_taxable_base_minor: projected,
        },
        origin,
        new_origin,
        fiscal_year_id,
        context,
        tax_status,
        vat_status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_swedish_clock_matches_independent_manifest_boundaries() {
        let manifest: Value = decode(CASH_V1.manifest).unwrap();
        let clocks = manifest["clockExpected"].as_array().unwrap();
        assert_eq!(clocks.len(), 8);
        for row in clocks {
            let now = row["utc"].as_str().unwrap().parse().unwrap();
            let day = row["swedishDate"].as_str().unwrap();
            ensure_not_future(day, now).unwrap();
            let next = date(day).unwrap().succ_opt().unwrap().to_string();
            assert!(ensure_not_future(&next, now).is_err(), "{row}");
        }
        assert_eq!(manifest["futureDayExpected"].as_array().unwrap().len(), 17);
        for row in manifest["futureDayExpected"].as_array().unwrap() {
            let now = row["nowUtc"].as_str().unwrap().parse().unwrap();
            assert_eq!(
                ensure_not_future(row["eventDate"].as_str().unwrap(), now).is_err(),
                row["isFuture"].as_bool().unwrap(),
                "{row}"
            );
        }
    }
}
