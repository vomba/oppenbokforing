use chrono::Utc;
use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::SqlitePool;

use crate::{
    error::AppError,
    ledger::{account_balance_minor, net_revenue_minor_for_fiscal_year},
    profiles::get_tax_profile,
    profiles::get_vat_profile,
    vat,
};

const PRELIMINARY_TAX_RATE_PERCENT: i64 = 30;

fn preliminary_tax_reserve_minor(base_minor: i64) -> i64 {
    (base_minor.max(0) * PRELIMINARY_TAX_RATE_PERCENT) / 100
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CashflowOverview {
    pub bank_balance_minor: i64,
    pub receivables_balance_minor: i64,
    pub vat_reserve_minor: i64,
    pub tax_reserve_minor: i64,
    pub spendable_cash_minor: i64,
    pub vat_period_key: Option<String>,
    pub threshold_warning: Option<String>,
}

pub async fn cashflow_overview_get(
    pool: &SqlitePool,
    workspace_id: &str,
    rule_year: i32,
) -> Result<CashflowOverview, AppError> {
    let bank_balance_minor = account_balance_minor(pool, workspace_id, "1930").await?;
    let receivables_balance_minor = account_balance_minor(pool, workspace_id, "1510").await?;
    let liquid_assets_minor = bank_balance_minor + receivables_balance_minor;

    let vat_profile = get_vat_profile(pool, workspace_id).await?;
    let vat_status = vat_profile
        .as_ref()
        .map(|p| p.vat_status.as_str())
        .unwrap_or("exempt_low_turnover");
    let reporting_period = vat_profile
        .as_ref()
        .map(|p| p.reporting_period.as_str())
        .unwrap_or("quarterly");
    let monitoring_period_key =
        vat::current_reporting_period_key(reporting_period, Utc::now().date_naive());

    let vat_reserve_minor = if vat_status == "registered" || vat_status == "voluntary_registered" {
        vat::estimated_vat_reserve_minor(pool, workspace_id, &monitoring_period_key).await?
    } else {
        0
    };

    let fiscal_year_id = format!("fy-{workspace_id}-{rule_year}");
    let ledger_profit =
        net_revenue_minor_for_fiscal_year(pool, workspace_id, &fiscal_year_id).await?;
    let business_tax_reserve = preliminary_tax_reserve_minor(ledger_profit);
    let tax_profile = get_tax_profile(pool, workspace_id).await?;
    let salary_tax_reserve = tax_profile
        .as_ref()
        .filter(|profile| profile.tax_status == "fa_skatt" && profile.expected_salary_income_minor > 0)
        .map(|profile| preliminary_tax_reserve_minor(profile.expected_salary_income_minor))
        .unwrap_or(0);
    let tax_reserve_minor = business_tax_reserve + salary_tax_reserve;

    let threshold = vat::vat_threshold_status(pool, workspace_id, rule_year).await?;
    let threshold_warning = if threshold.warning != "none" {
        Some(threshold.warning)
    } else {
        None
    };

    let spendable_cash_minor = liquid_assets_minor - vat_reserve_minor - tax_reserve_minor;

    Ok(CashflowOverview {
        bank_balance_minor,
        receivables_balance_minor,
        vat_reserve_minor,
        tax_reserve_minor,
        spendable_cash_minor,
        vat_period_key: Some(monitoring_period_key),
        threshold_warning,
    })
}
