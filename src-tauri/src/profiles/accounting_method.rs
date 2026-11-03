use sqlx::{Executor, Sqlite, Transaction};

use crate::error::AppError;

pub(crate) async fn ensure_invoice_timing_supported<'e, E>(
    executor: E,
    workspace_id: &str,
) -> Result<(), AppError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let method: Option<String> = sqlx::query_scalar(
        "SELECT accounting_method FROM vat_profiles WHERE workspace_id = ?1 LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(executor)
    .await?;
    match method.as_deref() {
        Some("invoice_method") => Ok(()),
        Some("cash_method") => Err(AppError::validation(
            "Cash-method posting and approval are unavailable until method-specific timing and reviewed legacy remediation are supported. Inspection and evidence retention remain available.",
            "accountingMethod",
        )),
        _ => Err(AppError::validation(
            "A supported accounting method is required before posting or approval. Review the business profile.",
            "accountingMethod",
        )),
    }
}

pub(super) async fn ensure_method_selection_allowed(
    tx: &mut Transaction<'_, Sqlite>,
    workspace_id: &str,
    accounting_method: &str,
) -> Result<(), AppError> {
    let changes_posted_method: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM vouchers WHERE workspace_id = ?1 AND status = 'posted'
        ) AND NOT EXISTS(
            SELECT 1 FROM vat_profiles WHERE workspace_id = ?1 AND accounting_method = ?2
        )
        "#,
    )
    .bind(workspace_id)
    .bind(accounting_method)
    .fetch_one(&mut **tx)
    .await?;
    if changes_posted_method {
        return Err(AppError::validation(
            "Accounting method cannot change through ordinary settings after posting. A reviewed method transition or legacy remediation is required; posted history must be preserved.",
            "accountingMethod",
        ));
    }
    Ok(())
}
