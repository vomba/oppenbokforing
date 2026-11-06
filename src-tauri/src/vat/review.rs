//! Canonical VAT review snapshot v1. See SPEC.md for the bound fields and
//! versioning contract; preserve deterministic object and row ordering here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Row, Sqlite, Transaction};

use super::{legacy_vat_return_trace_tx, VatReturnBox, VatReturnTrace, VatReturnTraceInput};
use crate::error::AppError;

type CanonicalRow = BTreeMap<String, Value>;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Source {
    identity: Vec<CanonicalRow>,
    vouchers: Vec<CanonicalRow>,
    lines: Vec<CanonicalRow>,
    accounts: Vec<CanonicalRow>,
    business_profiles: Vec<CanonicalRow>,
    tax_profiles: Vec<CanonicalRow>,
    vat_profiles: Vec<CanonicalRow>,
    active_rule_version: Vec<CanonicalRow>,
    rules: Vec<CanonicalRow>,
    vat_codes: Vec<CanonicalRow>,
    required_inputs: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Snapshot {
    version: u32,
    review_revision: String,
    source: Source,
    boxes: Vec<VatReturnBox>,
    pub(super) trace: VatReturnTrace,
}

pub(super) fn stale_review() -> AppError {
    AppError::validation(
        "VAT review is missing, unsupported, or stale. Regenerate the draft and review it before approving",
        "reviewedRevision",
    )
}

fn encode(snapshot: &Snapshot) -> Result<String, AppError> {
    serde_json::to_string(snapshot).map_err(|e| AppError::internal(e.to_string()))
}

fn hash(json: &str) -> String {
    format!("{:x}", Sha256::digest(json.as_bytes()))
}

async fn rows(
    tx: &mut Transaction<'_, Sqlite>,
    query: &str,
    workspace_id: &str,
    vat_return_id: &str,
) -> Result<Vec<CanonicalRow>, AppError> {
    // SQL JSON objects contain scalar values only. BTreeMap makes their order
    // independent of SQLite JSON key order and serde_json feature unification.
    sqlx::query_scalar::<_, String>(query)
        .bind(workspace_id)
        .bind(vat_return_id)
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|json| serde_json::from_str(&json).map_err(|e| AppError::internal(e.to_string())))
        .collect()
}

async fn source(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    vat_return_id: &str,
) -> Result<Source, AppError> {
    // Each query uses exactly the same period/workspace selection. Do not filter
    // lines to recognized VAT accounts: even equal-total/source-identity changes
    // must invalidate review. Voucher selection does not require any lines.
    let context = r#"
        WITH context AS (
            SELECT fp.starts_on, fp.ends_on, CAST(substr(fp.period_key, 1, 4) AS INTEGER) AS tax_year
            FROM vat_returns vr JOIN fiscal_periods fp ON fp.id = vr.fiscal_period_id
            WHERE vr.workspace_id = ?1 AND vr.id = ?2
        ), selected_vouchers AS (
            SELECT v.* FROM vouchers v, context c
            WHERE v.workspace_id = ?1 AND v.status = 'posted'
              AND date(COALESCE(v.accounting_date, v.posted_at)) >= date(c.starts_on)
              AND date(COALESCE(v.accounting_date, v.posted_at)) <= date(c.ends_on)
        ), active_version AS (
            SELECT rv.* FROM rule_versions rv, context c
            WHERE rv.tax_year = c.tax_year AND rv.status = 'active'
        )
    "#;
    let identity = rows(
        tx,
        r#"
        SELECT json_object(
            'workspaceId', w.id, 'workspaceActiveRuleYear', w.active_rule_year,
            'returnId', vr.id, 'returnRuleVersionId', vr.rule_version_id,
            'periodId', fp.id, 'periodWorkspaceId', fp.workspace_id,
            'periodFiscalYearId', fp.fiscal_year_id, 'periodKey', fp.period_key,
            'periodStartsOn', fp.starts_on, 'periodEndsOn', fp.ends_on,
            'fiscalYearId', fy.id, 'fiscalYearWorkspaceId', fy.workspace_id,
            'fiscalYearStartsOn', fy.starts_on, 'fiscalYearEndsOn', fy.ends_on)
        FROM vat_returns vr JOIN workspaces w ON w.id = vr.workspace_id
        JOIN fiscal_periods fp ON fp.id = vr.fiscal_period_id
        JOIN fiscal_years fy ON fy.id = fp.fiscal_year_id
        WHERE vr.workspace_id = ?1 AND vr.id = ?2 ORDER BY vr.id
    "#,
        workspace_id,
        vat_return_id,
    )
    .await?;
    let vouchers = rows(
        tx,
        &format!(
            r#"{context}
        SELECT json_object('id', id, 'workspaceId', workspace_id,
            'fiscalYearId', fiscal_year_id, 'status', status,
            'sourceType', source_type, 'sourceId', source_id,
            'accountingDate', accounting_date,
            'effectiveAccountingDate', date(COALESCE(accounting_date, posted_at)))
        FROM selected_vouchers ORDER BY id
    "#
        ),
        workspace_id,
        vat_return_id,
    )
    .await?;
    let lines = rows(
        tx,
        &format!(
            r#"{context}
        SELECT json_object('id', jl.id, 'voucherId', jl.voucher_id,
            'accountId', jl.account_id, 'debitMinor', jl.debit_minor,
            'creditMinor', jl.credit_minor, 'vatCode', jl.vat_code)
        FROM journal_lines jl JOIN selected_vouchers v ON v.id = jl.voucher_id
        ORDER BY jl.voucher_id, jl.id
    "#
        ),
        workspace_id,
        vat_return_id,
    )
    .await?;
    let accounts = rows(
        tx,
        &format!(
            r#"{context}
        SELECT json_object('id', a.id, 'workspaceId', a.workspace_id,
            'number', a.number, 'name', a.name, 'accountType', a.account_type,
            'normalBalance', a.normal_balance)
        FROM accounts a WHERE a.id IN (
            SELECT jl.account_id FROM journal_lines jl
            JOIN selected_vouchers v ON v.id = jl.voucher_id
        ) ORDER BY a.id
    "#
        ),
        workspace_id,
        vat_return_id,
    )
    .await?;
    // ?2 is deliberately used in these workspace-scoped queries as well, so
    // every helper has one consistent binding interface.
    let business_profiles = rows(
        tx,
        r#"
        SELECT json_object('businessName', business_name, 'ownerName', owner_name,
            'residencyCountry', residency_country, 'sniCode', sni_code)
        FROM sole_trader_profiles WHERE workspace_id = ?1 AND ?2 IS NOT NULL
        ORDER BY business_name, owner_name, residency_country, sni_code
    "#,
        workspace_id,
        vat_return_id,
    )
    .await?;
    let tax_profiles = rows(tx, r#"
        SELECT json_object('taxStatus', tax_status,
            'expectedBusinessProfitMinor', expected_business_profit_minor,
            'expectedSalaryIncomeMinor', expected_salary_income_minor,
            'activeRuleYear', active_rule_year)
        FROM tax_profiles WHERE workspace_id = ?1 AND ?2 IS NOT NULL
        ORDER BY tax_status, expected_business_profit_minor, expected_salary_income_minor, active_rule_year
    "#, workspace_id, vat_return_id).await?;
    let vat_profiles = rows(tx, r#"
        SELECT json_object('vatStatus', vat_status, 'reportingPeriod', reporting_period,
            'accountingMethod', accounting_method,
            'voluntaryRegistrationDate', voluntary_registration_date,
            'vatFilingDeadlineRegime', vat_filing_deadline_regime)
        FROM vat_profiles WHERE workspace_id = ?1 AND ?2 IS NOT NULL
        ORDER BY vat_status, reporting_period, accounting_method, voluntary_registration_date, vat_filing_deadline_regime
    "#, workspace_id, vat_return_id).await?;
    let active_rule_version = rows(tx, &format!(r#"{context}
        SELECT json_object('id', id, 'taxYear', tax_year, 'effectiveFrom', effective_from,
            'effectiveTo', effective_to, 'sourceUrl', source_url, 'checksum', checksum, 'status', status)
        FROM active_version ORDER BY id
    "#), workspace_id, vat_return_id).await?;
    let rules = rows(
        tx,
        &format!(
            r#"{context}
        SELECT json_object('id', tr.id, 'ruleVersionId', tr.rule_version_id,
            'family', tr.family, 'key', tr.key, 'valueJson', tr.value_json)
        FROM tax_rules tr JOIN active_version rv ON rv.id = tr.rule_version_id
        ORDER BY tr.family, tr.key, tr.id
    "#
        ),
        workspace_id,
        vat_return_id,
    )
    .await?;
    let vat_codes = rows(
        tx,
        r#"
        SELECT json_object('id', id, 'code', code, 'rateBp', rate_bp,
            'outputBox', output_box, 'inputBox', input_box, 'deductible', deductible)
        FROM vat_codes WHERE workspace_id = ?1 AND ?2 IS NOT NULL ORDER BY code, id
    "#,
        workspace_id,
        vat_return_id,
    )
    .await?;
    Ok(Source {
        identity,
        vouchers,
        lines,
        accounts,
        business_profiles,
        tax_profiles,
        vat_profiles,
        active_rule_version,
        rules,
        vat_codes,
        required_inputs: BTreeMap::new(),
    })
}

pub(super) async fn persist(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    vat_return_id: &str,
    revision: &str,
    boxes: &[VatReturnBox],
) -> Result<(), AppError> {
    let source = source(tx, workspace_id, vat_return_id).await?;
    let mut trace = legacy_vat_return_trace_tx(
        tx,
        workspace_id,
        &VatReturnTraceInput {
            vat_return_id: vat_return_id.to_string(),
        },
    )
    .await?;
    trace.review_revision = Some(revision.to_string());
    // Keep zero boxes in the trace too, even with no source vouchers.
    trace.boxes = boxes
        .iter()
        .map(|b| super::VatReturnBoxTrace {
            box_number: b.box_code.clone(),
            amount_minor: b.amount_minor,
            voucher_ids: trace
                .boxes
                .iter()
                .find(|t| t.box_number == b.box_code)
                .map(|t| t.voucher_ids.clone())
                .unwrap_or_default(),
        })
        .collect();
    let snapshot = Snapshot {
        version: 1,
        review_revision: revision.to_string(),
        source,
        boxes: boxes.to_vec(),
        trace,
    };
    let json = encode(&snapshot)?;
    sqlx::query("UPDATE vat_returns SET review_revision = ?1, review_snapshot_json = ?2, review_snapshot_hash = ?3 WHERE id = ?4 AND workspace_id = ?5")
        .bind(revision).bind(&json).bind(hash(&json)).bind(vat_return_id).bind(workspace_id)
        .execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn load(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    vat_return_id: &str,
) -> Result<Option<Snapshot>, AppError> {
    let row = sqlx::query("SELECT review_revision, review_snapshot_json, review_snapshot_hash FROM vat_returns WHERE workspace_id = ?1 AND id = ?2")
        .bind(workspace_id).bind(vat_return_id).fetch_optional(&mut **tx).await?
        .ok_or_else(|| AppError::validation("VAT return not found", "vatReturnId"))?;
    let revision: Option<String> = row.get("review_revision");
    let json: Option<String> = row.get("review_snapshot_json");
    let stored_hash: Option<String> = row.get("review_snapshot_hash");
    if revision.is_none() && json.is_none() && stored_hash.is_none() {
        return Ok(None);
    }
    let snapshot: Snapshot =
        serde_json::from_str(&json.ok_or_else(stale_review)?).map_err(|_| stale_review())?;
    if snapshot.version != 1
        || Some(snapshot.review_revision.as_str()) != revision.as_deref()
        || snapshot.trace.review_revision != revision
        || stored_hash.as_deref() != Some(hash(&encode(&snapshot)?).as_str())
    {
        return Err(stale_review());
    }
    Ok(Some(snapshot))
}

pub(super) async fn validate(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    vat_return_id: &str,
    revision: &str,
    displayed_boxes: &[VatReturnBox],
) -> Result<(), AppError> {
    let snapshot = load(tx, workspace_id, vat_return_id)
        .await?
        .ok_or_else(stale_review)?;
    if snapshot.review_revision != revision
        || snapshot.boxes != displayed_boxes
        || snapshot.source != source(tx, workspace_id, vat_return_id).await?
    {
        return Err(stale_review());
    }
    Ok(())
}
