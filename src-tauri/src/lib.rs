pub mod accountant_package;
pub mod audit;
pub mod backup;
pub mod bindings;
pub mod cashflow;
#[cfg(feature = "desktop")]
mod commands;
pub mod compliance;
pub mod counterparties;
pub mod db;
pub mod documents;
pub mod error;
pub mod expenses;
pub mod imports;
pub mod integrations;
pub mod invoicing;
pub mod jobs;
pub mod ledger;
pub mod paths;
pub mod profiles;
pub mod recent;
pub mod reconciliation;
pub mod rules;
pub mod settings;
pub mod sie;
pub mod state;
pub mod vat;
pub mod workspace;
pub mod year_end;

#[cfg(feature = "desktop")]
use commands::{
    business_profile_get_current, business_profile_save_current, compliance_check_run,
    compliance_profile_check,
    counterparty_create, counterparty_list, invoice_create_draft, invoice_credit, invoice_issue,
    invoice_list, invoice_open_count, invoice_pdf_status, invoice_update_draft, recent_workspaces_list,
    rule_version_get, tax_profile_get_current, tax_profile_save_current, vat_profile_get_current,
    vat_profile_save_current, workspace_backup_create, workspace_backup_restore, workspace_close,
    workspace_create, workspace_open,
    csv_import_create, document_import, document_reveal, expense_post, reconciliation_match_create,
    vat_return_draft_create, vat_return_get, vat_return_approve, vat_return_export,
    vat_threshold_status_get, cashflow_overview_get,
    year_end_package_create, year_end_package_get, year_end_package_find_by_fiscal_year,
    year_end_package_approve,
    year_end_readiness_get,
    year_end_package_export,
    workspace_settings_get, workspace_settings_save, dashboard_tour_mark_complete,
    sie_export_create,
    accountant_package_export_create, accountant_package_import_validate,
    integration_status_get,
    voucher_list, voucher_count, voucher_get, account_list, fiscal_period_list,
    staged_transactions_list, staged_transactions_count, document_list,
};
#[cfg(feature = "desktop")]
use state::AppState;

#[cfg(feature = "desktop")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            workspace_create,
            workspace_open,
            workspace_close,
            recent_workspaces_list,
            workspace_backup_create,
            workspace_backup_restore,
            business_profile_get_current,
            business_profile_save_current,
            tax_profile_get_current,
            tax_profile_save_current,
            vat_profile_get_current,
            vat_profile_save_current,
            compliance_check_run,
            compliance_profile_check,
            rule_version_get,
            counterparty_list,
            counterparty_create,
            invoice_list,
            invoice_create_draft,
            invoice_update_draft,
            invoice_issue,
            invoice_credit,
            invoice_open_count,
            invoice_pdf_status,
            document_import,
            document_reveal,
            expense_post,
            csv_import_create,
            reconciliation_match_create,
            vat_return_draft_create,
            vat_return_get,
            vat_return_approve,
            vat_return_export,
            vat_threshold_status_get,
            cashflow_overview_get,
            year_end_package_create,
            year_end_package_get,
            year_end_package_find_by_fiscal_year,
            year_end_package_approve,
            year_end_readiness_get,
            year_end_package_export,
            workspace_settings_get,
            workspace_settings_save,
            dashboard_tour_mark_complete,
            sie_export_create,
            accountant_package_export_create,
            accountant_package_import_validate,
            integration_status_get,
            voucher_list,
            voucher_count,
            voucher_get,
            account_list,
            fiscal_period_list,
            staged_transactions_list,
            staged_transactions_count,
            document_list,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(not(feature = "desktop"))]
pub fn run() {
    panic!("desktop feature is required to run the Tauri application");
}
