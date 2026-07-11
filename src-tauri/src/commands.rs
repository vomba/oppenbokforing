use sqlx::Row;
#[cfg(feature = "desktop")]
use tauri::{AppHandle, Manager, State};

use crate::{
    accountant_package::{
        self, AccountantPackageExportCreateInput, AccountantPackageExportSummary,
        AccountantPackageImportValidateInput, AccountantPackageValidateSummary,
    },
    backup::{
        self, BackupCreateInput, BackupRestoreInput, BackupRestoreSummary, BackupSummary,
    },
    bindings::{
        CommandResponse, ComplianceCheckInput, WorkspaceCreateInput, WorkspaceOpenInput,
        WorkspaceSummary,
    },
    compliance::{
        evaluate_scenario, run_profile_compliance_checks, ComplianceProfileCheckInput,
        ComplianceProfileCheckResult, ScenarioProfile, ScenarioTransaction,
    },
    counterparties::{self, Counterparty, CounterpartyCreateInput},
    db::{connect_workspace, open_existing_workspace},
    documents::{Document, DocumentGetInput, DocumentImportInput, DocumentListInput},
    error::AppError,
    expenses::{ExpensePostInput, ExpensePostResult},
    imports::{self, CsvImportCreateInput, CsvImportSummary, StagedTransactionsListInput, StagedTransactionSummary},
    integrations::{self, IntegrationStatusResponse},
    invoicing::{
        self, InvoiceCreateDraftInput, InvoiceCreditInput, InvoiceIssueInput, InvoiceListInput,
        InvoicePdfStatusInput, InvoiceSummary, InvoiceUpdateDraftInput,
    },
    ledger::{
        self, AccountSummary, VoucherCountInput, VoucherDetail, VoucherGetInput, VoucherListInput,
        VoucherSummary,
    },
    reconciliation::{self, ReconciliationMatchCreateInput, ReconciliationMatchResult},
    settings::{self, WorkspaceSettings, WorkspaceSettingsSaveInput},
    sie::{self, SieExportCreateInput, SieExportSummary},
    cashflow::{self, CashflowOverview},
    vat::{
        self, FiscalPeriodSummary, VatReturnApproveInput, VatReturnDraftCreateInput,
        VatReturnExportInput, VatReturnGetInput, VatReturnSummary, VatThresholdStatus,
    },
    year_end::{
        self, YearEndPackageApproveInput, YearEndPackageCreateInput, YearEndPackageExportInput,
        YearEndPackageFindInput, YearEndPackageGetInput, YearEndPackageSummary,
        YearEndReadiness, YearEndReadinessInput,
    },
    profiles::{
        self, BusinessProfile, BusinessProfileSaveInput, TaxProfile, TaxProfileSaveInput,
        VatProfile, VatProfileSaveInput,
    },
    recent::{self, RecentWorkspaceEntry},
    rules::get_active_rule_version,
    state::{AppState, WorkspaceContext},
    audit::record_event,
};

type CommandResult<T> = Result<CommandResponse<T>, AppError>;

async fn best_effort_process_invoice_pdf_jobs(
    pool: &sqlx::SqlitePool,
    workspace_id: &str,
) {
    if let Err(error) =
        crate::jobs::process_pending_invoice_pdf_jobs(pool, workspace_id).await
    {
        #[cfg(debug_assertions)]
        eprintln!("invoice PDF job processing failed: {error:?}");
    }
}

async fn require_workspace(state: &State<'_, AppState>) -> Result<WorkspaceContext, AppError> {
    let guard = state.current_workspace.lock().await;
    guard
        .clone()
        .ok_or_else(|| AppError::workspace_not_open("No workspace is open"))
}

fn app_data_dir(app: &AppHandle) -> Result<std::path::PathBuf, AppError> {
    app.path()
        .app_data_dir()
        .map_err(|error| AppError::storage(error.to_string()))
}

#[tauri::command]
pub async fn workspace_create(
    app: AppHandle,
    state: State<'_, AppState>,
    input: WorkspaceCreateInput,
) -> CommandResult<WorkspaceSummary> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(AppError::validation("Workspace name is required", "name"));
    }

    let workspace_id = uuid::Uuid::new_v4().to_string();
    let base_dir = app_data_dir(&app)?
        .join("workspaces")
        .join(&workspace_id);

    let documents_dir = base_dir.join("documents");
    let exports_dir = base_dir.join("exports");
    std::fs::create_dir_all(&documents_dir)?;
    std::fs::create_dir_all(&exports_dir)?;

    let database_path = base_dir.join("workspace.sqlite");
    let pool = connect_workspace(&database_path).await?;

    sqlx::query(
        r#"
        INSERT INTO workspaces (id, name, database_path, documents_path, exports_path)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(&workspace_id)
    .bind(name)
    .bind(database_path.to_string_lossy().to_string())
    .bind(documents_dir.to_string_lossy().to_string())
    .bind(exports_dir.to_string_lossy().to_string())
    .execute(&pool)
    .await?;

    crate::workspace::ensure_workspace_ready(&pool, &workspace_id).await?;

    record_event(
        &pool,
        &workspace_id,
        "workspace_create",
        "workspace",
        Some(&workspace_id),
        &serde_json::json!({ "name": name }).to_string(),
    )
    .await?;

    let context = WorkspaceContext {
        id: workspace_id.clone(),
        name: name.to_string(),
        data_dir: base_dir.clone(),
        database_path: database_path.clone(),
        pool,
    };

    let summary = workspace_summary_from_context(&context);
    *state.current_workspace.lock().await = Some(context);
    recent::record_recent_workspace(
        &app_data_dir(&app)?,
        &workspace_id,
        name,
        &summary.database_path,
    )?;

    Ok(CommandResponse { data: summary })
}

#[tauri::command]
pub async fn workspace_open(
    app: AppHandle,
    state: State<'_, AppState>,
    input: WorkspaceOpenInput,
) -> CommandResult<WorkspaceSummary> {
    let database_path = std::path::PathBuf::from(input.database_path.trim());
    if database_path.as_os_str().is_empty() {
        return Err(AppError::validation("Database path is required", "databasePath"));
    }

    let pool = open_existing_workspace(&database_path).await?;
    let row = sqlx::query(
        r#"
        SELECT id, name, database_path, documents_path, exports_path
        FROM workspaces
        WHERE database_path = ?1
        LIMIT 1
        "#,
    )
    .bind(database_path.to_string_lossy().to_string())
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::validation("Workspace not registered in database", "databasePath"))?;

    let workspace_id: String = row.get("id");
    let name: String = row.get("name");
    let documents_path: String = row.get("documents_path");

    let expected_documents = database_path
        .parent()
        .map(|parent| parent.join("documents"))
        .ok_or_else(|| AppError::storage("Invalid workspace database path"))?;
    if std::path::PathBuf::from(&documents_path) != expected_documents {
        return Err(AppError::validation(
            "Workspace documents path does not match database location",
            "databasePath",
        ));
    }

    let data_dir = expected_documents
        .parent()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| AppError::storage("Invalid workspace documents path"))?;

    record_event(
        &pool,
        &workspace_id,
        "workspace_open",
        "workspace",
        Some(&workspace_id),
        "{}",
    )
    .await?;

    let _ = crate::jobs::recover_stale_invoice_pdf_jobs(&pool, &workspace_id).await;
    let _ = crate::jobs::process_pending_invoice_pdf_jobs(&pool, &workspace_id).await;

    let context = WorkspaceContext {
        id: workspace_id.clone(),
        name: name.clone(),
        data_dir,
        database_path: database_path.clone(),
        pool,
    };

    let summary = workspace_summary_from_context(&context);
    *state.current_workspace.lock().await = Some(context);
    recent::record_recent_workspace(
        &app_data_dir(&app)?,
        &workspace_id,
        &name,
        &summary.database_path,
    )?;

    Ok(CommandResponse { data: summary })
}

#[tauri::command]
pub async fn workspace_close(state: State<'_, AppState>) -> CommandResult<bool> {
    let mut guard = state.current_workspace.lock().await;
    if let Some(context) = guard.as_ref() {
        record_event(
            &context.pool,
            &context.id,
            "workspace_close",
            "workspace",
            Some(&context.id),
            "{}",
        )
        .await?;
    }
    *guard = None;
    Ok(CommandResponse { data: true })
}

#[tauri::command]
pub async fn recent_workspaces_list(app: AppHandle) -> CommandResult<Vec<RecentWorkspaceEntry>> {
    let entries = recent::list_recent_workspaces(&app_data_dir(&app)?)?;
    Ok(CommandResponse { data: entries })
}

#[tauri::command]
pub async fn workspace_backup_create(
    app: AppHandle,
    state: State<'_, AppState>,
    input: BackupCreateInput,
) -> CommandResult<BackupSummary> {
    let workspace = require_workspace(&state).await?;
    match backup::claim_backup_create(
        &workspace.pool,
        &workspace.id,
        &input.idempotency_key,
        "workspace_backup_create",
    )
    .await?
    {
        backup::BackupCreateClaim::Cached(summary) => {
            if let Some(requested_path) = input.backup_file_path.as_deref() {
                if !backup::idempotent_backup_matches_request(&summary, Some(requested_path)) {
                    return Err(AppError::validation(
                        "Idempotency key was already used for a different backup destination",
                        "idempotencyKey",
                    ));
                }
            }
            return Ok(CommandResponse { data: summary });
        }
        backup::BackupCreateClaim::Proceed => {}
    }

    let destination = if input.backup_file_path.is_some() {
        workspace.data_dir.join("exports")
    } else if let Some(path) = input.destination_path.as_deref() {
        crate::paths::validate_user_directory(path, "destinationPath")?
    } else if let Some(path) =
        crate::paths::resolve_backup_destination(&workspace.pool, &workspace.id, None).await?
    {
        path
    } else {
        workspace.data_dir.join("exports")
    };

    let summary = backup::create_backup_package(
        &workspace.pool,
        &workspace.id,
        &workspace.data_dir,
        &workspace.database_path,
        &destination,
        &input.passphrase,
        input.backup_file_path.as_deref(),
    )
    .await?;

    backup::finalize_backup_create(
        &workspace.pool,
        &workspace.id,
        &input.idempotency_key,
        "workspace_backup_create",
        &summary,
    )
    .await?;

    let _ = app;
    Ok(CommandResponse { data: summary })
}

#[tauri::command]
pub async fn workspace_backup_restore(
    app: AppHandle,
    input: BackupRestoreInput,
) -> CommandResult<BackupRestoreSummary> {
    let workspaces_root = app_data_dir(&app)?.join("workspaces");
    std::fs::create_dir_all(&workspaces_root)?;
    let summary = backup::restore_backup_package(&input, &workspaces_root).await?;
    Ok(CommandResponse { data: summary })
}

#[tauri::command]
pub async fn business_profile_get_current(
    state: State<'_, AppState>,
) -> CommandResult<BusinessProfile> {
    let workspace = require_workspace(&state).await?;
    let profile = profiles::get_business_profile(&workspace.pool, &workspace.id)
        .await?
        .ok_or_else(|| AppError::validation("Business profile not found", "businessProfile"))?;
    Ok(CommandResponse { data: profile })
}

#[tauri::command]
pub async fn business_profile_save_current(
    state: State<'_, AppState>,
    input: BusinessProfileSaveInput,
) -> CommandResult<BusinessProfile> {
    let workspace = require_workspace(&state).await?;
    let profile =
        profiles::save_business_profile(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: profile })
}

#[tauri::command]
pub async fn tax_profile_get_current(state: State<'_, AppState>) -> CommandResult<TaxProfile> {
    let workspace = require_workspace(&state).await?;
    let profile = profiles::get_tax_profile(&workspace.pool, &workspace.id)
        .await?
        .ok_or_else(|| AppError::validation("Tax profile not found", "taxProfile"))?;
    Ok(CommandResponse { data: profile })
}

#[tauri::command]
pub async fn vat_profile_get_current(state: State<'_, AppState>) -> CommandResult<VatProfile> {
    let workspace = require_workspace(&state).await?;
    let profile = profiles::get_vat_profile(&workspace.pool, &workspace.id)
        .await?
        .ok_or_else(|| AppError::validation("VAT profile not found", "vatProfile"))?;
    Ok(CommandResponse { data: profile })
}

#[tauri::command]
pub async fn tax_profile_save_current(
    state: State<'_, AppState>,
    input: TaxProfileSaveInput,
) -> CommandResult<TaxProfile> {
    let workspace = require_workspace(&state).await?;
    let profile = profiles::save_tax_profile(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: profile })
}

#[tauri::command]
pub async fn vat_profile_save_current(
    state: State<'_, AppState>,
    input: VatProfileSaveInput,
) -> CommandResult<VatProfile> {
    let workspace = require_workspace(&state).await?;
    let profile = profiles::save_vat_profile(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: profile })
}

#[tauri::command]
pub async fn compliance_check_run(
    state: State<'_, AppState>,
    input: ComplianceCheckInput,
) -> CommandResult<crate::compliance::ComplianceCheckResult> {
    let workspace = require_workspace(&state).await?;

    let scenario = crate::state::load_golden_scenario(&input.scenario_id);
    let profile: ScenarioProfile = serde_json::from_value(scenario.profile)
        .map_err(|error| AppError::validation(error.to_string(), "scenarioId"))?;
    let transactions: Vec<ScenarioTransaction> = scenario
        .transactions
        .iter()
        .map(|value| serde_json::from_value(value.clone()))
        .collect::<Result<_, _>>()
        .map_err(|error| AppError::validation(error.to_string(), "scenarioId"))?;

    let tax_profile = profiles::get_tax_profile(&workspace.pool, &workspace.id).await?;
    let vat_profile = profiles::get_vat_profile(&workspace.pool, &workspace.id).await?;
    let merged_profile = merge_profile_with_workspace(profile, tax_profile, vat_profile);

    let result =
        evaluate_scenario(&workspace.pool, &input.scenario_id, &merged_profile, &transactions)
            .await?;

    record_event(
        &workspace.pool,
        &workspace.id,
        "compliance_check_run",
        "scenario",
        Some(&input.scenario_id),
        &serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string()),
    )
    .await?;

    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn compliance_profile_check(
    state: State<'_, AppState>,
    input: ComplianceProfileCheckInput,
) -> CommandResult<ComplianceProfileCheckResult> {
    let workspace = require_workspace(&state).await?;
    let result = run_profile_compliance_checks(&workspace.pool, &input).await?;

    record_event(
        &workspace.pool,
        &workspace.id,
        "compliance_profile_check",
        "scenario",
        None,
        &serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string()),
    )
    .await?;

    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn rule_version_get(state: State<'_, AppState>) -> CommandResult<crate::rules::RuleVersionSummary> {
    let workspace = require_workspace(&state).await?;
    let rule = get_active_rule_version(&workspace.pool)
        .await?
        .ok_or_else(|| AppError::validation("No active rule version", "ruleVersion"))?;
    Ok(CommandResponse { data: rule })
}

#[tauri::command]
pub async fn counterparty_list(state: State<'_, AppState>) -> CommandResult<Vec<Counterparty>> {
    let workspace = require_workspace(&state).await?;
    crate::workspace::ensure_workspace_ready(&workspace.pool, &workspace.id).await?;
    let items = counterparties::list_counterparties(&workspace.pool, &workspace.id).await?;
    Ok(CommandResponse { data: items })
}

#[tauri::command]
pub async fn counterparty_create(
    state: State<'_, AppState>,
    input: CounterpartyCreateInput,
) -> CommandResult<Counterparty> {
    let workspace = require_workspace(&state).await?;
    crate::workspace::ensure_workspace_ready(&workspace.pool, &workspace.id).await?;
    let item = counterparties::create_counterparty(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: item })
}

#[tauri::command]
pub async fn invoice_list(
    state: State<'_, AppState>,
    input: InvoiceListInput,
) -> CommandResult<Vec<InvoiceSummary>> {
    let workspace = require_workspace(&state).await?;
    crate::workspace::ensure_workspace_ready(&workspace.pool, &workspace.id).await?;
    let items = invoicing::list_invoices(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: items })
}

#[tauri::command]
pub async fn invoice_create_draft(
    state: State<'_, AppState>,
    input: InvoiceCreateDraftInput,
) -> CommandResult<InvoiceSummary> {
    let workspace = require_workspace(&state).await?;
    crate::workspace::ensure_workspace_ready(&workspace.pool, &workspace.id).await?;
    let invoice =
        invoicing::create_draft(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: invoice })
}

#[tauri::command]
pub async fn invoice_update_draft(
    state: State<'_, AppState>,
    input: InvoiceUpdateDraftInput,
) -> CommandResult<InvoiceSummary> {
    let workspace = require_workspace(&state).await?;
    let invoice =
        invoicing::update_draft(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: invoice })
}

#[tauri::command]
pub async fn invoice_issue(
    state: State<'_, AppState>,
    input: InvoiceIssueInput,
) -> CommandResult<InvoiceSummary> {
    let workspace = require_workspace(&state).await?;
    let invoice = invoicing::issue_invoice(&workspace.pool, &workspace.id, &input).await?;
    best_effort_process_invoice_pdf_jobs(&workspace.pool, &workspace.id).await;
    Ok(CommandResponse { data: invoice })
}

#[tauri::command]
pub async fn invoice_credit(
    state: State<'_, AppState>,
    input: InvoiceCreditInput,
) -> CommandResult<InvoiceSummary> {
    let workspace = require_workspace(&state).await?;
    let invoice = invoicing::credit_invoice(&workspace.pool, &workspace.id, &input).await?;
    best_effort_process_invoice_pdf_jobs(&workspace.pool, &workspace.id).await;
    Ok(CommandResponse { data: invoice })
}

#[tauri::command]
pub async fn invoice_open_count(state: State<'_, AppState>) -> CommandResult<i64> {
    let workspace = require_workspace(&state).await?;
    let count = invoicing::count_open_invoices(&workspace.pool, &workspace.id).await?;
    Ok(CommandResponse { data: count })
}

#[tauri::command]
pub async fn invoice_pdf_status(
    state: State<'_, AppState>,
    input: InvoicePdfStatusInput,
) -> CommandResult<String> {
    let workspace = require_workspace(&state).await?;
    crate::jobs::process_pending_invoice_pdf_jobs(&workspace.pool, &workspace.id).await?;
    let invoice =
        invoicing::get_invoice(&workspace.pool, &workspace.id, &input.invoice_id).await?;
    let status =
        crate::jobs::invoice_pdf_status(&workspace.pool, &workspace.id, &invoice).await?;
    Ok(CommandResponse { data: status })
}

#[tauri::command]
pub async fn document_reveal(
    state: State<'_, AppState>,
    input: DocumentGetInput,
) -> CommandResult<bool> {
    let workspace = require_workspace(&state).await?;
    crate::documents::document_reveal(&workspace.pool, &workspace.id, &input.document_id).await?;
    Ok(CommandResponse { data: true })
}

#[tauri::command]
pub async fn document_import(
    state: State<'_, AppState>,
    input: DocumentImportInput,
) -> CommandResult<Document> {
    let workspace = require_workspace(&state).await?;
    crate::workspace::ensure_workspace_ready(&workspace.pool, &workspace.id).await?;
    let doc = crate::documents::document_import(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: doc })
}

#[tauri::command]
pub async fn expense_post(
    state: State<'_, AppState>,
    input: ExpensePostInput,
) -> CommandResult<ExpensePostResult> {
    let workspace = require_workspace(&state).await?;
    crate::workspace::ensure_workspace_ready(&workspace.pool, &workspace.id).await?;
    let result = crate::expenses::expense_post(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn csv_import_create(
    state: State<'_, AppState>,
    input: CsvImportCreateInput,
) -> CommandResult<CsvImportSummary> {
    let workspace = require_workspace(&state).await?;
    crate::workspace::ensure_workspace_ready(&workspace.pool, &workspace.id).await?;
    let result = imports::csv_import_create(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn reconciliation_match_create(
    state: State<'_, AppState>,
    input: ReconciliationMatchCreateInput,
) -> CommandResult<ReconciliationMatchResult> {
    let workspace = require_workspace(&state).await?;
    crate::workspace::ensure_workspace_ready(&workspace.pool, &workspace.id).await?;
    let result =
        reconciliation::reconciliation_match_create(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn vat_return_draft_create(
    state: State<'_, AppState>,
    input: VatReturnDraftCreateInput,
) -> CommandResult<VatReturnSummary> {
    let workspace = require_workspace(&state).await?;
    crate::workspace::ensure_workspace_ready(&workspace.pool, &workspace.id).await?;
    let result = vat::vat_return_draft_create(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn vat_return_get(
    state: State<'_, AppState>,
    input: VatReturnGetInput,
) -> CommandResult<VatReturnSummary> {
    let workspace = require_workspace(&state).await?;
    let result = vat::vat_return_get(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn vat_return_approve(
    state: State<'_, AppState>,
    input: VatReturnApproveInput,
) -> CommandResult<VatReturnSummary> {
    let workspace = require_workspace(&state).await?;
    let result = vat::vat_return_approve(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn vat_return_export(
    state: State<'_, AppState>,
    input: VatReturnExportInput,
) -> CommandResult<VatReturnSummary> {
    let workspace = require_workspace(&state).await?;
    let result = vat::vat_return_export(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn vat_threshold_status_get(state: State<'_, AppState>) -> CommandResult<VatThresholdStatus> {
    let workspace = require_workspace(&state).await?;
    let tax_profile = profiles::get_tax_profile(&workspace.pool, &workspace.id).await?;
    let rule_year = tax_profile
        .map(|p| p.active_rule_year)
        .unwrap_or(2026);
    let status = vat::vat_threshold_status(&workspace.pool, &workspace.id, rule_year).await?;
    Ok(CommandResponse { data: status })
}

#[tauri::command]
pub async fn cashflow_overview_get(state: State<'_, AppState>) -> CommandResult<CashflowOverview> {
    let workspace = require_workspace(&state).await?;
    let tax_profile = profiles::get_tax_profile(&workspace.pool, &workspace.id).await?;
    let rule_year = tax_profile
        .map(|p| p.active_rule_year)
        .unwrap_or(2026);
    let overview =
        cashflow::cashflow_overview_get(&workspace.pool, &workspace.id, rule_year).await?;
    Ok(CommandResponse { data: overview })
}

#[tauri::command]
pub async fn year_end_package_create(
    state: State<'_, AppState>,
    input: YearEndPackageCreateInput,
) -> CommandResult<YearEndPackageSummary> {
    let workspace = require_workspace(&state).await?;
    crate::workspace::ensure_workspace_ready(&workspace.pool, &workspace.id).await?;
    let result = year_end::year_end_package_create(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn year_end_package_get(
    state: State<'_, AppState>,
    input: YearEndPackageGetInput,
) -> CommandResult<YearEndPackageSummary> {
    let workspace = require_workspace(&state).await?;
    let result = year_end::year_end_package_get(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn year_end_package_find_by_fiscal_year(
    state: State<'_, AppState>,
    input: YearEndPackageFindInput,
) -> CommandResult<Option<YearEndPackageSummary>> {
    let workspace = require_workspace(&state).await?;
    let result =
        year_end::year_end_package_find_by_fiscal_year(&workspace.pool, &workspace.id, &input)
            .await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn year_end_package_approve(
    state: State<'_, AppState>,
    input: YearEndPackageApproveInput,
) -> CommandResult<YearEndPackageSummary> {
    let workspace = require_workspace(&state).await?;
    let result = year_end::year_end_package_approve(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn year_end_package_export(
    state: State<'_, AppState>,
    input: YearEndPackageExportInput,
) -> CommandResult<YearEndPackageSummary> {
    let workspace = require_workspace(&state).await?;
    let result = year_end::year_end_package_export(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn workspace_settings_get(
    state: State<'_, AppState>,
) -> CommandResult<WorkspaceSettings> {
    let workspace = require_workspace(&state).await?;
    let result = settings::workspace_settings_get(&workspace.pool, &workspace.id).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn workspace_settings_save(
    state: State<'_, AppState>,
    input: WorkspaceSettingsSaveInput,
) -> CommandResult<WorkspaceSettings> {
    let workspace = require_workspace(&state).await?;
    let result = settings::workspace_settings_save(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn dashboard_tour_mark_complete(
    state: State<'_, AppState>,
) -> CommandResult<WorkspaceSettings> {
    let workspace = require_workspace(&state).await?;
    let result = settings::dashboard_tour_mark_complete(&workspace.pool, &workspace.id).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn sie_export_create(
    state: State<'_, AppState>,
    input: SieExportCreateInput,
) -> CommandResult<SieExportSummary> {
    let workspace = require_workspace(&state).await?;
    let result = sie::sie_export_create(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn accountant_package_export_create(
    state: State<'_, AppState>,
    input: AccountantPackageExportCreateInput,
) -> CommandResult<AccountantPackageExportSummary> {
    let workspace = require_workspace(&state).await?;
    let result =
        accountant_package::accountant_package_export_create(&workspace.pool, &workspace.id, &input)
            .await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn accountant_package_import_validate(
    state: State<'_, AppState>,
    input: AccountantPackageImportValidateInput,
) -> CommandResult<AccountantPackageValidateSummary> {
    let workspace = require_workspace(&state).await?;
    let result = accountant_package::accountant_package_import_validate(
        &workspace.pool,
        &workspace.id,
        &input,
    )
    .await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn integration_status_get() -> CommandResult<IntegrationStatusResponse> {
    Ok(CommandResponse {
        data: integrations::status_response(),
    })
}

#[tauri::command]
pub async fn voucher_list(
    state: State<'_, AppState>,
    input: VoucherListInput,
) -> CommandResult<Vec<VoucherSummary>> {
    let workspace = require_workspace(&state).await?;
    let result = ledger::voucher_list(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn voucher_count(
    state: State<'_, AppState>,
    input: VoucherCountInput,
) -> CommandResult<i64> {
    let workspace = require_workspace(&state).await?;
    let result = ledger::voucher_count(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn voucher_get(
    state: State<'_, AppState>,
    input: VoucherGetInput,
) -> CommandResult<VoucherDetail> {
    let workspace = require_workspace(&state).await?;
    let result = ledger::voucher_get(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn account_list(state: State<'_, AppState>) -> CommandResult<Vec<AccountSummary>> {
    let workspace = require_workspace(&state).await?;
    let result = ledger::account_list(&workspace.pool, &workspace.id).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn fiscal_period_list(
    state: State<'_, AppState>,
) -> CommandResult<Vec<FiscalPeriodSummary>> {
    let workspace = require_workspace(&state).await?;
    let result = vat::fiscal_period_list(&workspace.pool, &workspace.id).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn staged_transactions_list(
    state: State<'_, AppState>,
    input: StagedTransactionsListInput,
) -> CommandResult<Vec<StagedTransactionSummary>> {
    let workspace = require_workspace(&state).await?;
    let result = imports::staged_transactions_list(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn staged_transactions_count(
    state: State<'_, AppState>,
    status: Option<String>,
) -> CommandResult<i64> {
    let workspace = require_workspace(&state).await?;
    let count = imports::staged_transactions_count(
        &workspace.pool,
        &workspace.id,
        status.as_deref(),
    )
    .await?;
    Ok(CommandResponse { data: count })
}

#[tauri::command]
pub async fn year_end_readiness_get(
    state: State<'_, AppState>,
    input: YearEndReadinessInput,
) -> CommandResult<YearEndReadiness> {
    let workspace = require_workspace(&state).await?;
    let result = year_end::year_end_readiness_get(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

#[tauri::command]
pub async fn document_list(
    state: State<'_, AppState>,
    input: DocumentListInput,
) -> CommandResult<Vec<Document>> {
    let workspace = require_workspace(&state).await?;
    let result = crate::documents::document_list(&workspace.pool, &workspace.id, &input).await?;
    Ok(CommandResponse { data: result })
}

fn merge_profile_with_workspace(
    mut profile: ScenarioProfile,
    tax_profile: Option<TaxProfile>,
    vat_profile: Option<VatProfile>,
) -> ScenarioProfile {
    if let Some(tax) = tax_profile {
        profile.tax_status = Some(tax.tax_status);
        profile.expected_business_profit_minor = Some(tax.expected_business_profit_minor);
        profile.expected_salary_income_minor = Some(tax.expected_salary_income_minor);
        profile.rule_year = Some(tax.active_rule_year);
    }
    if let Some(vat) = vat_profile {
        profile.vat_status = Some(vat.vat_status);
    }
    profile
}

fn workspace_summary_from_context(context: &WorkspaceContext) -> WorkspaceSummary {
    WorkspaceSummary {
        id: context.id.clone(),
        name: context.name.clone(),
        data_dir: context.data_dir.to_string_lossy().to_string(),
        database_path: context.database_path.to_string_lossy().to_string(),
    }
}
