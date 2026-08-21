use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::{Row, SqlitePool};

use crate::{
    error::AppError,
    profiles::get_vat_profile,
    rules::{get_active_rule_version, RuleVersionSummary},
};

const VAT_DEADLINE_RULE_KEY: &str = "filing_deadline_schedules";
const INCOME_DEADLINE_RULE_KEY: &str = "income_declaration_deadline";

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaxTaskListInput {
    pub as_of_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaxTask {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub target: String,
    pub period_key: String,
    pub due_on: Option<String>,
    pub rule_version_id: String,
    pub tax_year: i32,
    pub source_url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VatDeadlineSchedules {
    source_url: String,
    schedules: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IncomeDeadline {
    source_url: String,
    due_on: String,
}

#[derive(Debug)]
struct ExistingReturn {
    status: String,
    export_path: Option<String>,
}

fn parse_as_of_date(value: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Invalid as-of date format", "asOfDate"))
}


fn regime_matches_reporting_period(reporting_period: &str, regime: &str) -> bool {
    matches!(
        (reporting_period, regime),
        ("yearly", "annual_may_12" | "annual_feb_26")
            | ("quarterly", "quarterly_12")
            | ("monthly", "monthly_12" | "monthly_26")
    )
}

fn task_status_for_due(due_on: NaiveDate, as_of: NaiveDate, needs_work: bool) -> String {
    if due_on < as_of {
        "overdue".to_string()
    } else if needs_work {
        "action_required".to_string()
    } else {
        "upcoming".to_string()
    }
}

fn task_metadata(rule_version: &RuleVersionSummary, source_url: String) -> (String, i32, String) {
    (rule_version.id.clone(), rule_version.tax_year, source_url)
}

async fn active_rule_value(
    pool: &SqlitePool,
    rule_version_id: &str,
    family: &str,
    key: &str,
) -> Result<Option<String>, AppError> {
    sqlx::query_scalar(
        r#"
        SELECT value_json
        FROM tax_rules
        WHERE rule_version_id = ?1 AND family = ?2 AND key = ?3
        LIMIT 1
        "#,
    )
    .bind(rule_version_id)
    .bind(family)
    .bind(key)
    .fetch_optional(pool)
    .await
    .map_err(AppError::from)
}

async fn existing_vat_return(
    pool: &SqlitePool,
    workspace_id: &str,
    period_key: &str,
) -> Result<Option<ExistingReturn>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT vr.status, vr.export_path
        FROM vat_returns vr
        JOIN fiscal_periods fp ON fp.id = vr.fiscal_period_id
        WHERE vr.workspace_id = ?1 AND fp.period_key = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(period_key)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| ExistingReturn {
        status: row.get("status"),
        export_path: row.get("export_path"),
    }))
}

fn profile_review_task(rule_version: &RuleVersionSummary) -> TaxTask {
    TaxTask {
        id: "profile_review:vat_filing_deadline_regime".to_string(),
        kind: "profile_review".to_string(),
        status: "date_unavailable".to_string(),
        target: "onboarding".to_string(),
        period_key: rule_version.tax_year.to_string(),
        due_on: None,
        rule_version_id: rule_version.id.clone(),
        tax_year: rule_version.tax_year,
        source_url: rule_version.source_url.clone(),
    }
}

async fn vat_tasks(
    pool: &SqlitePool,
    workspace_id: &str,
    as_of: NaiveDate,
    rule_version: &RuleVersionSummary,
) -> Result<Vec<TaxTask>, AppError> {
    let Some(profile) = get_vat_profile(pool, workspace_id).await? else {
        return Ok(vec![profile_review_task(rule_version)]);
    };
    if profile.vat_status == "exempt_low_turnover" {
        return Ok(Vec::new());
    }
    let Some(regime) = profile.vat_filing_deadline_regime.as_deref() else {
        return Ok(vec![profile_review_task(rule_version)]);
    };
    if !regime_matches_reporting_period(&profile.reporting_period, regime) {
        return Ok(vec![profile_review_task(rule_version)]);
    }

    let Some(raw_rule) = active_rule_value(pool, &rule_version.id, "vat", VAT_DEADLINE_RULE_KEY).await? else {
        return Ok(vec![profile_review_task(rule_version)]);
    };
    let schedules: VatDeadlineSchedules = serde_json::from_str(&raw_rule)
        .map_err(|_| AppError::validation("VAT deadline rules are invalid", "ruleVersion"))?;
    let Some(schedule) = schedules.schedules.get(regime) else {
        return Ok(vec![profile_review_task(rule_version)]);
    };

    let mut deadlines = schedule
        .iter()
        .map(|(period_key, due_on)| {
            NaiveDate::parse_from_str(due_on, "%Y-%m-%d")
                .map(|due_date| (period_key, due_on, due_date))
                .map_err(|_| AppError::validation("VAT deadline rules are invalid", "ruleVersion"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    deadlines.sort_by_key(|(_, _, due_date)| *due_date);
    let next_due = deadlines
        .iter()
        .find(|(_, _, due_date)| *due_date > as_of)
        .map(|(period_key, _, _)| (*period_key).to_string());
    let (rule_version_id, tax_year, source_url) = task_metadata(rule_version, schedules.source_url);

    let mut tasks = Vec::new();
    for (period_key, due_on, due_date) in deadlines {
        if due_date > as_of && next_due.as_deref() != Some(period_key.as_str()) {
            continue;
        }
        let existing = existing_vat_return(pool, workspace_id, period_key).await?;
        let status = match existing {
            Some(existing) if existing.status == "approved" && existing.export_path.is_some() => {
                "prepared_external_submission_required".to_string()
            }
            Some(_) => task_status_for_due(due_date, as_of, true),
            None => task_status_for_due(due_date, as_of, false),
        };
        tasks.push(TaxTask {
            id: format!("vat_return:{period_key}"),
            kind: "vat_return".to_string(),
            status,
            target: "vat".to_string(),
            period_key: period_key.to_string(),
            due_on: Some(due_on.to_string()),
            rule_version_id: rule_version_id.clone(),
            tax_year,
            source_url: source_url.clone(),
        });
    }
    Ok(tasks)
}

async fn year_end_task(
    pool: &SqlitePool,
    workspace_id: &str,
    as_of: NaiveDate,
    rule_version: &RuleVersionSummary,
) -> Result<Option<TaxTask>, AppError> {
    let Some(raw_rule) = active_rule_value(pool, &rule_version.id, "year_end", INCOME_DEADLINE_RULE_KEY).await? else {
        return Ok(Some(profile_review_task(rule_version)));
    };
    let deadline: IncomeDeadline = serde_json::from_str(&raw_rule)
        .map_err(|_| AppError::validation("Income declaration deadline rule is invalid", "ruleVersion"))?;
    let due_date = NaiveDate::parse_from_str(&deadline.due_on, "%Y-%m-%d")
        .map_err(|_| AppError::validation("Income declaration deadline rule is invalid", "ruleVersion"))?;
    let fiscal_year = rule_version.tax_year - 1;
    let package: Option<(String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT yep.status, yep.export_path
        FROM year_end_packages yep
        JOIN fiscal_years fy ON fy.id = yep.fiscal_year_id
        WHERE yep.workspace_id = ?1 AND fy.starts_on = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(format!("{fiscal_year}-01-01"))
    .fetch_optional(pool)
    .await?;
    let status = match package {
        Some((status, Some(_))) if status == "approved" => {
            "prepared_external_submission_required".to_string()
        }
        _ => task_status_for_due(due_date, as_of, true),
    };
    let (rule_version_id, tax_year, source_url) = task_metadata(rule_version, deadline.source_url);

    Ok(Some(TaxTask {
        id: format!("year_end:{fiscal_year}"),
        kind: "year_end".to_string(),
        status,
        target: "year_end".to_string(),
        period_key: fiscal_year.to_string(),
        due_on: Some(deadline.due_on),
        rule_version_id,
        tax_year,
        source_url,
    }))
}

fn task_sort_key(task: &TaxTask) -> (u8, String, String) {
    let status_rank = match task.status.as_str() {
        "overdue" => 0,
        "action_required" => 1,
        "upcoming" => 2,
        "prepared_external_submission_required" => 3,
        _ => 4,
    };
    (status_rank, task.due_on.clone().unwrap_or_default(), task.id.clone())
}

pub async fn tax_task_list(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &TaxTaskListInput,
) -> Result<Vec<TaxTask>, AppError> {
    let as_of = parse_as_of_date(input.as_of_date.trim())?;
    let Some(rule_version) = get_active_rule_version(pool).await? else {
        return Err(AppError::validation("No active tax rules are available", "ruleVersion"));
    };

    let mut tasks = vat_tasks(pool, workspace_id, as_of, &rule_version).await?;
    if let Some(task) = year_end_task(pool, workspace_id, as_of, &rule_version).await? {
        tasks.push(task);
    }
    tasks.sort_by_key(task_sort_key);
    Ok(tasks)
}
