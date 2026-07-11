use serde::{Deserialize, Serialize};
use specta::Type;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{audit::record_event, error::AppError};

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BusinessProfile {
    pub id: String,
    pub business_name: String,
    pub owner_name: String,
    pub residency_country: String,
    pub sni_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BusinessProfileSaveInput {
    pub business_name: String,
    pub owner_name: String,
    pub residency_country: Option<String>,
    pub sni_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaxProfile {
    pub id: String,
    pub tax_status: String,
    pub expected_business_profit_minor: i64,
    pub expected_salary_income_minor: i64,
    pub active_rule_year: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaxProfileSaveInput {
    pub tax_status: String,
    pub expected_business_profit_minor: Option<i64>,
    pub expected_salary_income_minor: Option<i64>,
    pub active_rule_year: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatProfile {
    pub id: String,
    pub vat_status: String,
    pub reporting_period: String,
    pub accounting_method: String,
    pub voluntary_registration_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VatProfileSaveInput {
    pub vat_status: String,
    pub reporting_period: String,
    pub accounting_method: String,
    pub voluntary_registration_date: Option<String>,
}

pub async fn get_business_profile(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Option<BusinessProfile>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, business_name, owner_name, residency_country, sni_code
        FROM sole_trader_profiles
        WHERE workspace_id = ?1
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| BusinessProfile {
        id: row.get("id"),
        business_name: row.get("business_name"),
        owner_name: row.get("owner_name"),
        residency_country: row.get("residency_country"),
        sni_code: row.get("sni_code"),
    }))
}

pub async fn save_business_profile(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &BusinessProfileSaveInput,
) -> Result<BusinessProfile, AppError> {
    let business_name = input.business_name.trim();
    let owner_name = input.owner_name.trim();
    if business_name.is_empty() {
        return Err(AppError::validation("Business name is required", "businessName"));
    }
    if owner_name.is_empty() {
        return Err(AppError::validation("Owner name is required", "ownerName"));
    }

    let residency = input
        .residency_country
        .as_deref()
        .unwrap_or("SE")
        .trim()
        .to_uppercase();
    if residency.len() != 2 {
        return Err(AppError::validation(
            "Residency country must be a 2-letter code",
            "residencyCountry",
        ));
    }

    let existing = get_business_profile(pool, workspace_id).await?;
    let profile_id = existing
        .as_ref()
        .map(|profile| profile.id.clone())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    if existing.is_some() {
        sqlx::query(
            r#"
            UPDATE sole_trader_profiles
            SET business_name = ?1,
                owner_name = ?2,
                residency_country = ?3,
                sni_code = ?4,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?5
            "#,
        )
        .bind(business_name)
        .bind(owner_name)
        .bind(&residency)
        .bind(input.sni_code.as_deref())
        .bind(&profile_id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            r#"
            INSERT INTO sole_trader_profiles (
              id, workspace_id, business_name, owner_name, residency_country, sni_code
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(&profile_id)
        .bind(workspace_id)
        .bind(business_name)
        .bind(owner_name)
        .bind(&residency)
        .bind(input.sni_code.as_deref())
        .execute(pool)
        .await?;
    }

    let profile = BusinessProfile {
        id: profile_id.clone(),
        business_name: business_name.to_string(),
        owner_name: owner_name.to_string(),
        residency_country: residency,
        sni_code: input.sni_code.clone(),
    };

    record_event(
        pool,
        workspace_id,
        "business_profile_save_current",
        "sole_trader_profile",
        Some(&profile_id),
        &serde_json::to_string(&profile).unwrap_or_else(|_| "{}".to_string()),
    )
    .await?;

    Ok(profile)
}

pub async fn save_tax_profile(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &TaxProfileSaveInput,
) -> Result<TaxProfile, AppError> {
    let tax_status = input.tax_status.trim();
    if !matches!(tax_status, "planning" | "f_skatt" | "fa_skatt") {
        return Err(AppError::validation("Invalid tax status", "taxStatus"));
    }

    let business_profit = input.expected_business_profit_minor.unwrap_or(0);
    let salary_income = input.expected_salary_income_minor.unwrap_or(0);
    let rule_year = input.active_rule_year.unwrap_or(2026);

    if tax_status == "fa_skatt" && salary_income <= 0 {
        return Err(AppError::validation(
            "FA-skatt profile requires expected salary income",
            "expectedSalaryIncomeMinor",
        ));
    }

    let existing = sqlx::query(
        r#"
        SELECT id FROM tax_profiles WHERE workspace_id = ?1 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    let profile_id = existing
        .as_ref()
        .map(|row| row.get::<String, _>("id"))
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    if existing.is_some() {
        sqlx::query(
            r#"
            UPDATE tax_profiles
            SET tax_status = ?1,
                expected_business_profit_minor = ?2,
                expected_salary_income_minor = ?3,
                active_rule_year = ?4,
                updated_at = CURRENT_TIMESTAMP
            WHERE workspace_id = ?5
            "#,
        )
        .bind(tax_status)
        .bind(business_profit)
        .bind(salary_income)
        .bind(rule_year)
        .bind(workspace_id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            r#"
            INSERT INTO tax_profiles (
              id, workspace_id, tax_status,
              expected_business_profit_minor, expected_salary_income_minor, active_rule_year
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(&profile_id)
        .bind(workspace_id)
        .bind(tax_status)
        .bind(business_profit)
        .bind(salary_income)
        .bind(rule_year)
        .execute(pool)
        .await?;
    }

    let profile = TaxProfile {
        id: profile_id.clone(),
        tax_status: tax_status.to_string(),
        expected_business_profit_minor: business_profit,
        expected_salary_income_minor: salary_income,
        active_rule_year: rule_year,
    };

    record_event(
        pool,
        workspace_id,
        "tax_profile_save_current",
        "tax_profile",
        Some(&profile_id),
        &serde_json::to_string(&profile).unwrap_or_else(|_| "{}".to_string()),
    )
    .await?;

    Ok(profile)
}

pub async fn save_vat_profile(
    pool: &SqlitePool,
    workspace_id: &str,
    input: &VatProfileSaveInput,
) -> Result<VatProfile, AppError> {
    let vat_status = input.vat_status.trim();
    if !matches!(
        vat_status,
        "registered" | "exempt_low_turnover" | "voluntary_registered"
    ) {
        return Err(AppError::validation("Invalid VAT status", "vatStatus"));
    }

    let reporting_period = input.reporting_period.trim();
    if !matches!(reporting_period, "monthly" | "quarterly" | "yearly") {
        return Err(AppError::validation("Invalid reporting period", "reportingPeriod"));
    }

    let accounting_method = input.accounting_method.trim();
    if !matches!(accounting_method, "invoice_method" | "cash_method") {
        return Err(AppError::validation("Invalid accounting method", "accountingMethod"));
    }

    let existing = sqlx::query(
        r#"
        SELECT id FROM vat_profiles WHERE workspace_id = ?1 LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    let profile_id = existing
        .as_ref()
        .map(|row| row.get::<String, _>("id"))
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    if existing.is_some() {
        sqlx::query(
            r#"
            UPDATE vat_profiles
            SET vat_status = ?1,
                reporting_period = ?2,
                accounting_method = ?3,
                voluntary_registration_date = ?4,
                updated_at = CURRENT_TIMESTAMP
            WHERE workspace_id = ?5
            "#,
        )
        .bind(vat_status)
        .bind(reporting_period)
        .bind(accounting_method)
        .bind(input.voluntary_registration_date.as_deref())
        .bind(workspace_id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            r#"
            INSERT INTO vat_profiles (
              id, workspace_id, vat_status, reporting_period, accounting_method,
              voluntary_registration_date
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(&profile_id)
        .bind(workspace_id)
        .bind(vat_status)
        .bind(reporting_period)
        .bind(accounting_method)
        .bind(input.voluntary_registration_date.as_deref())
        .execute(pool)
        .await?;
    }

    let profile = VatProfile {
        id: profile_id.clone(),
        vat_status: vat_status.to_string(),
        reporting_period: reporting_period.to_string(),
        accounting_method: accounting_method.to_string(),
        voluntary_registration_date: input.voluntary_registration_date.clone(),
    };

    record_event(
        pool,
        workspace_id,
        "vat_profile_save_current",
        "vat_profile",
        Some(&profile_id),
        &serde_json::to_string(&profile).unwrap_or_else(|_| "{}".to_string()),
    )
    .await?;

    Ok(profile)
}

pub async fn get_tax_profile(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Option<TaxProfile>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, tax_status, expected_business_profit_minor,
               expected_salary_income_minor, active_rule_year
        FROM tax_profiles
        WHERE workspace_id = ?1
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| TaxProfile {
        id: row.get("id"),
        tax_status: row.get("tax_status"),
        expected_business_profit_minor: row.get("expected_business_profit_minor"),
        expected_salary_income_minor: row.get("expected_salary_income_minor"),
        active_rule_year: row.get("active_rule_year"),
    }))
}

pub async fn get_vat_profile(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<Option<VatProfile>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT id, vat_status, reporting_period, accounting_method, voluntary_registration_date
        FROM vat_profiles
        WHERE workspace_id = ?1
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| VatProfile {
        id: row.get("id"),
        vat_status: row.get("vat_status"),
        reporting_period: row.get("reporting_period"),
        accounting_method: row.get("accounting_method"),
        voluntary_registration_date: row.get("voluntary_registration_date"),
    }))
}
