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
        "SELECT CASE WHEN EXISTS(SELECT 1 FROM cash_invoice_issues WHERE workspace_id=?1) THEN 'cash_method' ELSE accounting_method END FROM vat_profiles WHERE workspace_id=?1 LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(executor)
    .await?;
    match method.as_deref() {
        Some("invoice_method") => Ok(()),
        Some("cash_method") => Err(AppError::validation(
            "Cash-method invoice-timed recognition and approval are unavailable on this route. Only separately reviewed cash transitions may post; legacy timing requires reviewed remediation. Inspection and retained evidence remain available.",
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
    // Historical-schema callers retain the old freeze. Normal workspace opening
    // always migrates first; a damaged schema-22 table still fails closed.
    let cash_schema: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=202611040002 AND success=1)",
    )
    .fetch_one(&mut **tx)
    .await?;
    let cash_identity: bool = if cash_schema {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cash_invoice_issues WHERE workspace_id=?1)")
            .bind(workspace_id)
            .fetch_one(&mut **tx)
            .await?
    } else {
        false
    };
    if cash_identity && accounting_method != "cash_method" {
        return Err(AppError::validation(
            "Accounting method cannot change after issued cash identity, even before receipt posting. A reviewed method transition is required.",
            "accountingMethod",
        ));
    }
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
