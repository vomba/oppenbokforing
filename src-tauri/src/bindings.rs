use specta::Type;

use crate::{
    accountant_package::{
        AccountantPackageEntry, AccountantPackageExportCreateInput,
        AccountantPackageExportSummary, AccountantPackageImportValidateInput,
        AccountantPackageManifest, AccountantPackageValidateSummary,
    },
    backup::{
        BackupCreateInput, BackupManifest, BackupManifestEntry, BackupRestoreInput,
        BackupRestoreSummary, BackupSummary,
    },
    compliance::ComplianceCheckResult,
    counterparties::{Counterparty, CounterpartyCreateInput},
    documents::{Document, DocumentGetInput, DocumentImportInput, DocumentListInput},
    error::{AppError, FieldError},
    expenses::{ExpensePostInput, ExpensePostResult},
    imports::{CsvImportCreateInput, CsvImportSummary, StagedTransactionSummary, StagedTransactionsListInput},
    integrations::IntegrationStatusResponse,
    invoicing::{
        InvoiceCreateDraftInput, InvoiceCreditInput, InvoiceIssueInput, InvoiceLine,
        InvoiceLineInput, InvoiceListInput, InvoicePdfStatusInput, InvoiceSummary, InvoiceUpdateDraftInput,
    },
    ledger::{
        AccountSummary, JournalLineRow, VoucherCountInput, VoucherDetail, VoucherGetInput,
        VoucherListInput,
        VoucherSummary,
    },
    reconciliation::{ReconciliationMatchCreateInput, ReconciliationMatchResult},
    cashflow::CashflowOverview,
    vat::{
        FiscalPeriodSummary, VatReturnApproveInput, VatReturnDraftCreateInput, VatReturnExportInput,
        VatReturnGetInput, VatReturnSummary, VatThresholdStatus, VatReturnBox,
    },
    year_end::{
        NeFieldSummary, YearEndPackageApproveInput, YearEndPackageCreateInput,
        YearEndPackageExportInput, YearEndPackageFindInput, YearEndPackageGetInput,
        YearEndPackageSummary, YearEndReadiness, YearEndReadinessInput, YearEndReadinessItem,
    },
    profiles::{
        BusinessProfile, BusinessProfileSaveInput, TaxProfile, TaxProfileSaveInput, VatProfile,
        VatProfileSaveInput,
    },
    recent::RecentWorkspaceEntry,
    rules::RuleVersionSummary,
    settings::{WorkspaceSettings, WorkspaceSettingsSaveInput},
    sie::{SieExportCreateInput, SieExportSummary},
};

#[derive(Debug, Clone, serde::Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSummary {
    pub id: String,
    pub name: String,
    pub data_dir: String,
    pub database_path: String,
}

#[derive(Debug, Clone, serde::Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCreateInput {
    pub name: String,
}

#[derive(Debug, Clone, serde::Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceOpenInput {
    pub database_path: String,
}

#[derive(Debug, Clone, serde::Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceCheckInput {
    pub scenario_id: String,
}

#[derive(Debug, Clone, serde::Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandResponse<T> {
    pub data: T,
}

pub fn export_types() -> String {
    let mut types = specta::TypeCollection::default();
    types.register::<WorkspaceSummary>();
    types.register::<WorkspaceCreateInput>();
    types.register::<WorkspaceOpenInput>();
    types.register::<ComplianceCheckInput>();
    types.register::<ComplianceCheckResult>();
    types.register::<RuleVersionSummary>();
    types.register::<BusinessProfile>();
    types.register::<BusinessProfileSaveInput>();
    types.register::<TaxProfile>();
    types.register::<TaxProfileSaveInput>();
    types.register::<VatProfile>();
    types.register::<VatProfileSaveInput>();
    types.register::<BackupCreateInput>();
    types.register::<BackupRestoreInput>();
    types.register::<BackupSummary>();
    types.register::<BackupRestoreSummary>();
    types.register::<BackupManifest>();
    types.register::<BackupManifestEntry>();
    types.register::<RecentWorkspaceEntry>();
    types.register::<Counterparty>();
    types.register::<CounterpartyCreateInput>();
    types.register::<InvoiceLineInput>();
    types.register::<InvoiceLine>();
    types.register::<InvoiceSummary>();
    types.register::<InvoiceCreateDraftInput>();
    types.register::<InvoiceUpdateDraftInput>();
    types.register::<InvoiceIssueInput>();
    types.register::<InvoiceCreditInput>();
    types.register::<InvoiceListInput>();
    types.register::<InvoicePdfStatusInput>();
    types.register::<VoucherListInput>();
    types.register::<VoucherCountInput>();
    types.register::<VoucherGetInput>();
    types.register::<VoucherSummary>();
    types.register::<VoucherDetail>();
    types.register::<JournalLineRow>();
    types.register::<AccountSummary>();
    types.register::<FiscalPeriodSummary>();
    types.register::<DocumentImportInput>();
    types.register::<DocumentListInput>();
    types.register::<DocumentGetInput>();
    types.register::<Document>();
    types.register::<ExpensePostInput>();
    types.register::<ExpensePostResult>();
    types.register::<CsvImportCreateInput>();
    types.register::<CsvImportSummary>();
    types.register::<StagedTransactionsListInput>();
    types.register::<StagedTransactionSummary>();
    types.register::<ReconciliationMatchCreateInput>();
    types.register::<ReconciliationMatchResult>();
    types.register::<VatReturnBox>();
    types.register::<VatReturnSummary>();
    types.register::<VatReturnDraftCreateInput>();
    types.register::<VatReturnGetInput>();
    types.register::<VatReturnApproveInput>();
    types.register::<VatReturnExportInput>();
    types.register::<VatThresholdStatus>();
    types.register::<CashflowOverview>();
    types.register::<NeFieldSummary>();
    types.register::<YearEndPackageSummary>();
    types.register::<YearEndPackageCreateInput>();
    types.register::<YearEndPackageGetInput>();
    types.register::<YearEndPackageFindInput>();
    types.register::<YearEndPackageApproveInput>();
    types.register::<YearEndPackageExportInput>();
    types.register::<YearEndReadinessInput>();
    types.register::<YearEndReadinessItem>();
    types.register::<YearEndReadiness>();
    types.register::<WorkspaceSettings>();
    types.register::<WorkspaceSettingsSaveInput>();
    types.register::<SieExportCreateInput>();
    types.register::<SieExportSummary>();
    types.register::<AccountantPackageEntry>();
    types.register::<AccountantPackageManifest>();
    types.register::<AccountantPackageExportCreateInput>();
    types.register::<AccountantPackageExportSummary>();
    types.register::<AccountantPackageImportValidateInput>();
    types.register::<AccountantPackageValidateSummary>();
    types.register::<IntegrationStatusResponse>();
    types.register::<crate::integrations::IntegrationStatus>();
    types.register::<AppError>();
    types.register::<FieldError>();
    types.register::<CommandResponse<WorkspaceSummary>>();
    types.register::<CommandResponse<bool>>();
    types.register::<CommandResponse<ComplianceCheckResult>>();
    types.register::<CommandResponse<RuleVersionSummary>>();
    types.register::<CommandResponse<BusinessProfile>>();
    types.register::<CommandResponse<TaxProfile>>();
    types.register::<CommandResponse<VatProfile>>();
    types.register::<CommandResponse<BackupSummary>>();
    types.register::<CommandResponse<BackupRestoreSummary>>();
    types.register::<CommandResponse<Vec<RecentWorkspaceEntry>>>();
    types.register::<CommandResponse<Vec<Counterparty>>>();
    types.register::<CommandResponse<Counterparty>>();
    types.register::<CommandResponse<Vec<InvoiceSummary>>>();
    types.register::<CommandResponse<InvoiceSummary>>();
    types.register::<CommandResponse<i64>>();
    types.register::<CommandResponse<Document>>();
    types.register::<CommandResponse<ExpensePostResult>>();
    types.register::<CommandResponse<CsvImportSummary>>();
    types.register::<CommandResponse<ReconciliationMatchResult>>();
    types.register::<CommandResponse<VatReturnSummary>>();
    types.register::<CommandResponse<VatThresholdStatus>>();
    types.register::<CommandResponse<CashflowOverview>>();
    types.register::<CommandResponse<YearEndPackageSummary>>();
    types.register::<CommandResponse<YearEndReadiness>>();
    types.register::<CommandResponse<Option<YearEndPackageSummary>>>();
    types.register::<CommandResponse<WorkspaceSettings>>();
    types.register::<CommandResponse<SieExportSummary>>();
    types.register::<CommandResponse<AccountantPackageExportSummary>>();
    types.register::<CommandResponse<AccountantPackageValidateSummary>>();
    types.register::<CommandResponse<IntegrationStatusResponse>>();
    types.register::<CommandResponse<Vec<VoucherSummary>>>();
    types.register::<CommandResponse<VoucherDetail>>();
    types.register::<CommandResponse<Vec<AccountSummary>>>();
    types.register::<CommandResponse<Vec<FiscalPeriodSummary>>>();
    types.register::<CommandResponse<Vec<StagedTransactionSummary>>>();
    types.register::<CommandResponse<Vec<Document>>>();

    let temp_path = std::env::temp_dir().join("oppenbokforing-bindings.ts");
    specta_typescript::Typescript::default()
        .bigint(specta_typescript::BigIntExportBehavior::Number)
        .export_to(&temp_path, &types)
        .expect("failed to export TypeScript bindings");
    std::fs::read_to_string(&temp_path).expect("failed to read exported bindings")
}
