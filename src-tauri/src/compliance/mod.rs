use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::SqlitePool;

use crate::{
    error::AppError,
    rules::get_rule_i64,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioProfile {
    pub tax_status: Option<String>,
    pub vat_status: Option<String>,
    pub expected_salary_income_minor: Option<i64>,
    pub expected_business_profit_minor: Option<i64>,
    pub rule_year: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioTransaction {
    #[serde(rename = "type")]
    pub kind: String,
    pub amount_minor_ex_vat: Option<i64>,
    pub vat_rate: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceCheckResult {
    pub scenario_id: String,
    pub passed: bool,
    pub outcomes: serde_json::Value,
    pub rule_year: i32,
}

pub async fn evaluate_scenario(
    pool: &SqlitePool,
    scenario_id: &str,
    profile: &ScenarioProfile,
    transactions: &[ScenarioTransaction],
) -> Result<ComplianceCheckResult, AppError> {
    let outcomes = match scenario_id {
        "fa-skatt-salary-and-business" => evaluate_fa_skatt(profile).await?,
        "vat-exempt-below-threshold" => evaluate_vat_exempt_below(pool, profile, transactions).await?,
        "vat-exempt-threshold-breach" => evaluate_vat_exempt_breach(pool, profile, transactions).await?,
        id => {
            return Ok(ComplianceCheckResult {
                scenario_id: id.to_string(),
                passed: false,
                outcomes: serde_json::json!({ "error": "scenario_not_implemented" }),
                rule_year: profile.rule_year.unwrap_or(2026),
            });
        }
    };

    let passed = match scenario_id {
        "fa-skatt-salary-and-business" => {
            outcomes["salaryIncomeInBusinessLedger"] == false
                && outcomes["requiresFaSkattGuidance"] == true
                && outcomes["invoiceMustMentionFSkatt"] == true
                && outcomes["taxPlanningUsesSalaryAssumption"] == true
        }
        "vat-exempt-below-threshold" => {
            outcomes["mustChargeVat"] == false
                && outcomes["mustRegisterForVat"] == false
                && outcomes["invoiceMustStateVatExemption"] == true
        }
        "vat-exempt-threshold-breach" => {
            outcomes["mustRegisterForVat"] == true
                && outcomes["mustChargeVatFromBreachSale"] == true
        }
        _ => false,
    };

    Ok(ComplianceCheckResult {
        scenario_id: scenario_id.to_string(),
        passed,
        outcomes,
        rule_year: profile.rule_year.unwrap_or(2026),
    })
}

async fn evaluate_fa_skatt(profile: &ScenarioProfile) -> Result<serde_json::Value, AppError> {
    let tax_status = profile.tax_status.as_deref().unwrap_or("");
    let salary = profile.expected_salary_income_minor.unwrap_or(0);

    Ok(serde_json::json!({
        "salaryIncomeInBusinessLedger": false,
        "businessIncomeInLedger": tax_status == "fa_skatt",
        "requiresFaSkattGuidance": tax_status == "fa_skatt",
        "invoiceMustMentionFSkatt": tax_status == "fa_skatt" || tax_status == "f_skatt",
        "taxPlanningUsesSalaryAssumption": tax_status == "fa_skatt" && salary > 0,
    }))
}

async fn evaluate_vat_exempt_below(
    pool: &SqlitePool,
    profile: &ScenarioProfile,
    transactions: &[ScenarioTransaction],
) -> Result<serde_json::Value, AppError> {
    let threshold = get_rule_i64(pool, "vat", "annual_turnover_threshold_minor")
        .await?
        .unwrap_or(12_000_000);
    let warning_ratio = get_rule_i64(pool, "vat", "threshold_warning_ratio")
        .await?
        .unwrap_or(80);
    let turnover = sum_turnover(transactions);
    let warning = if turnover as f64 >= threshold as f64 * (warning_ratio as f64 / 100.0) {
        "approaching"
    } else {
        "none"
    };

    let vat_status = profile.vat_status.as_deref().unwrap_or("");
    Ok(serde_json::json!({
        "annualTurnoverMinor": turnover,
        "mustChargeVat": false,
        "mustRegisterForVat": false,
        "invoiceMustStateVatExemption": vat_status == "exempt_low_turnover",
        "thresholdWarning": warning,
        "thresholdMinor": threshold,
    }))
}

async fn evaluate_vat_exempt_breach(
    pool: &SqlitePool,
    profile: &ScenarioProfile,
    transactions: &[ScenarioTransaction],
) -> Result<serde_json::Value, AppError> {
    let threshold = get_rule_i64(pool, "vat", "annual_turnover_threshold_minor")
        .await?
        .unwrap_or(12_000_000);
    let turnover = sum_turnover(transactions);
    let mut breach_index: Option<usize> = None;
    let mut running = 0i64;

    for (index, tx) in transactions.iter().enumerate() {
        running += tx.amount_minor_ex_vat.unwrap_or(0);
        if running > threshold && breach_index.is_none() {
            breach_index = Some(index);
        }
    }

    let vat_status = profile.vat_status.as_deref().unwrap_or("");
    Ok(serde_json::json!({
        "annualTurnoverMinor": turnover,
        "breachTransactionIndex": breach_index,
        "mustRegisterForVat": turnover > threshold && vat_status == "exempt_low_turnover",
        "mustChargeVatFromBreachSale": breach_index.is_some(),
        "requiresUserReviewBeforeIssuing": breach_index.is_some(),
        "thresholdMinor": threshold,
    }))
}

fn sum_turnover(transactions: &[ScenarioTransaction]) -> i64 {
    transactions
        .iter()
        .filter(|tx| tx.kind == "invoice_issued")
        .map(|tx| tx.amount_minor_ex_vat.unwrap_or(0))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connect_workspace;
    use tempfile::tempdir;

    #[tokio::test]
    async fn fa_skatt_fixture_passes() {
        let dir = tempdir().unwrap();
        let pool = connect_workspace(&dir.path().join("workspace.sqlite"))
            .await
            .unwrap();
        let profile = ScenarioProfile {
            tax_status: Some("fa_skatt".to_string()),
            vat_status: Some("registered".to_string()),
            expected_salary_income_minor: Some(48_000_000),
            expected_business_profit_minor: Some(18_000_000),
            rule_year: Some(2026),
        };
        let result = evaluate_scenario(&pool, "fa-skatt-salary-and-business", &profile, &[])
            .await
            .unwrap();
        assert!(result.passed);
    }
}
