import { invoke } from "@tauri-apps/api/core"
import type {
  BackupCreateInput,
  BackupRestoreInput,
  BackupRestoreSummary,
  BackupSummary,
  BusinessProfile,
  BusinessProfileSaveInput,
  ComplianceCheckInput,
  ComplianceCheckResult,
  Counterparty,
  CounterpartyCreateInput,
  CsvImportCreateInput,
  CsvImportSummary,
  Document,
  DocumentGetInput,
  DocumentImportInput,
  DocumentListInput,
  AccountSummary,
  ExpensePostInput,
  ExpensePostResult,
  FiscalPeriodSummary,
  InvoiceCreateDraftInput,
  InvoiceCreditInput,
  InvoiceIssueInput,
  InvoiceListInput,
  InvoicePdfStatusInput,
  InvoiceSummary,
  InvoiceUpdateDraftInput,
  RecentWorkspaceEntry,
  ReconciliationMatchCreateInput,
  ReconciliationMatchResult,
  StagedTransactionSummary,
  StagedTransactionsListInput,
  RuleVersionSummary,
  TaxProfile,
  TaxProfileSaveInput,
  VatProfile,
  VatProfileSaveInput,
  VatReturnApproveInput,
  VatReturnDraftCreateInput,
  VatReturnExportInput,
  VatReturnGetInput,
  VatReturnSummary,
  VatThresholdStatus,
  VoucherCountInput,
  VoucherDetail,
  VoucherGetInput,
  VoucherListInput,
  VoucherSummary,
  CashflowOverview,
  YearEndPackageApproveInput,
  YearEndPackageCreateInput,
  YearEndPackageExportInput,
  YearEndPackageFindInput,
  YearEndPackageGetInput,
  YearEndPackageSummary,
  YearEndReadiness,
  YearEndReadinessInput,
  WorkspaceCreateInput,
  WorkspaceOpenInput,
  WorkspaceSummary,
  WorkspaceSettings,
  WorkspaceSettingsSaveInput,
  SieExportCreateInput,
  SieExportSummary,
  AccountantPackageExportCreateInput,
  AccountantPackageExportSummary,
  AccountantPackageImportValidateInput,
  AccountantPackageValidateSummary,
  IntegrationStatusResponse,
} from "./bindings"

export type {
  AppError,
  BackupCreateInput,
  BackupRestoreInput,
  BackupRestoreSummary,
  BackupSummary,
  BusinessProfile,
  BusinessProfileSaveInput,
  ComplianceCheckInput,
  ComplianceCheckResult,
  Counterparty,
  CounterpartyCreateInput,
  CsvImportCreateInput,
  CsvImportSummary,
  Document,
  DocumentGetInput,
  DocumentImportInput,
  DocumentListInput,
  AccountSummary,
  ExpensePostInput,
  ExpensePostResult,
  FiscalPeriodSummary,
  InvoiceCreateDraftInput,
  InvoiceCreditInput,
  InvoiceIssueInput,
  InvoiceListInput,
  InvoicePdfStatusInput,
  InvoiceSummary,
  InvoiceUpdateDraftInput,
  RecentWorkspaceEntry,
  ReconciliationMatchCreateInput,
  ReconciliationMatchResult,
  StagedTransactionSummary,
  StagedTransactionsListInput,
  RuleVersionSummary,
  TaxProfile,
  TaxProfileSaveInput,
  VatProfile,
  VatProfileSaveInput,
  VatReturnApproveInput,
  VatReturnDraftCreateInput,
  VatReturnExportInput,
  VatReturnGetInput,
  VatReturnSummary,
  VatThresholdStatus,
  VoucherCountInput,
  VoucherDetail,
  VoucherGetInput,
  VoucherListInput,
  VoucherSummary,
  CashflowOverview,
  YearEndPackageApproveInput,
  YearEndPackageCreateInput,
  YearEndPackageExportInput,
  YearEndPackageFindInput,
  YearEndPackageGetInput,
  YearEndPackageSummary,
  YearEndReadiness,
  YearEndReadinessInput,
  WorkspaceCreateInput,
  WorkspaceOpenInput,
  WorkspaceSummary,
  WorkspaceSettings,
  WorkspaceSettingsSaveInput,
  SieExportCreateInput,
  SieExportSummary,
  AccountantPackageExportCreateInput,
  AccountantPackageExportSummary,
  AccountantPackageImportValidateInput,
  AccountantPackageValidateSummary,
  IntegrationStatusResponse,
} from "./bindings"

type CommandResponse<T> = {
  data: T
}

export async function workspaceCreate(input: WorkspaceCreateInput) {
  const response = await invoke<CommandResponse<WorkspaceSummary>>("workspace_create", { input })
  return response.data
}

export async function workspaceOpen(input: WorkspaceOpenInput) {
  const response = await invoke<CommandResponse<WorkspaceSummary>>("workspace_open", { input })
  return response.data
}

export async function workspaceClose() {
  const response = await invoke<CommandResponse<boolean>>("workspace_close")
  return response.data
}

export async function recentWorkspacesList() {
  const response = await invoke<CommandResponse<RecentWorkspaceEntry[]>>("recent_workspaces_list")
  return response.data
}

export async function workspaceBackupCreate(input: BackupCreateInput) {
  const response = await invoke<CommandResponse<BackupSummary>>("workspace_backup_create", { input })
  return response.data
}

export async function workspaceBackupRestore(input: BackupRestoreInput) {
  const response = await invoke<CommandResponse<BackupRestoreSummary>>("workspace_backup_restore", {
    input,
  })
  return response.data
}

export async function businessProfileGetCurrent() {
  const response = await invoke<CommandResponse<BusinessProfile>>("business_profile_get_current")
  return response.data
}

export async function businessProfileSaveCurrent(input: BusinessProfileSaveInput) {
  const response = await invoke<CommandResponse<BusinessProfile>>("business_profile_save_current", {
    input,
  })
  return response.data
}

export async function taxProfileGetCurrent() {
  const response = await invoke<CommandResponse<TaxProfile>>("tax_profile_get_current")
  return response.data
}

export async function vatProfileGetCurrent() {
  const response = await invoke<CommandResponse<VatProfile>>("vat_profile_get_current")
  return response.data
}

export async function taxProfileSaveCurrent(input: TaxProfileSaveInput) {
  const response = await invoke<CommandResponse<TaxProfile>>("tax_profile_save_current", { input })
  return response.data
}

export async function vatProfileSaveCurrent(input: VatProfileSaveInput) {
  const response = await invoke<CommandResponse<VatProfile>>("vat_profile_save_current", { input })
  return response.data
}

export async function complianceCheckRun(input: ComplianceCheckInput) {
  const response = await invoke<CommandResponse<ComplianceCheckResult>>("compliance_check_run", {
    input,
  })
  return response.data
}

export async function ruleVersionGet() {
  const response = await invoke<CommandResponse<RuleVersionSummary>>("rule_version_get")
  return response.data
}

export async function counterpartyList() {
  const response = await invoke<CommandResponse<Counterparty[]>>("counterparty_list")
  return response.data
}

export async function counterpartyCreate(input: CounterpartyCreateInput) {
  const response = await invoke<CommandResponse<Counterparty>>("counterparty_create", { input })
  return response.data
}

export async function invoiceList(input: InvoiceListInput) {
  const response = await invoke<CommandResponse<InvoiceSummary[]>>("invoice_list", { input })
  return response.data
}

export async function invoiceCreateDraft(input: InvoiceCreateDraftInput) {
  const response = await invoke<CommandResponse<InvoiceSummary>>("invoice_create_draft", { input })
  return response.data
}

export async function invoiceUpdateDraft(input: InvoiceUpdateDraftInput) {
  const response = await invoke<CommandResponse<InvoiceSummary>>("invoice_update_draft", { input })
  return response.data
}

export async function invoiceIssue(input: InvoiceIssueInput) {
  const response = await invoke<CommandResponse<InvoiceSummary>>("invoice_issue", { input })
  return response.data
}

export async function invoiceCredit(input: InvoiceCreditInput) {
  const response = await invoke<CommandResponse<InvoiceSummary>>("invoice_credit", { input })
  return response.data
}

export async function invoiceOpenCount() {
  const response = await invoke<CommandResponse<number>>("invoice_open_count")
  return response.data
}

export async function invoicePdfStatus(input: InvoicePdfStatusInput) {
  const response = await invoke<CommandResponse<string>>("invoice_pdf_status", { input })
  return response.data
}

export async function documentImport(input: DocumentImportInput) {
  const response = await invoke<CommandResponse<Document>>("document_import", { input })
  return response.data
}

export async function expensePost(input: ExpensePostInput) {
  const response = await invoke<CommandResponse<ExpensePostResult>>("expense_post", { input })
  return response.data
}

export async function csvImportCreate(input: CsvImportCreateInput) {
  const response = await invoke<CommandResponse<CsvImportSummary>>("csv_import_create", { input })
  return response.data
}

export async function reconciliationMatchCreate(input: ReconciliationMatchCreateInput) {
  const response = await invoke<CommandResponse<ReconciliationMatchResult>>(
    "reconciliation_match_create",
    { input },
  )
  return response.data
}

export async function vatReturnDraftCreate(input: VatReturnDraftCreateInput) {
  const response = await invoke<CommandResponse<VatReturnSummary>>("vat_return_draft_create", {
    input,
  })
  return response.data
}

export async function vatReturnGet(input: VatReturnGetInput) {
  const response = await invoke<CommandResponse<VatReturnSummary>>("vat_return_get", { input })
  return response.data
}

export async function vatReturnApprove(input: VatReturnApproveInput) {
  const response = await invoke<CommandResponse<VatReturnSummary>>("vat_return_approve", { input })
  return response.data
}

export async function vatReturnExport(input: VatReturnExportInput) {
  const response = await invoke<CommandResponse<VatReturnSummary>>("vat_return_export", { input })
  return response.data
}

export async function vatThresholdStatusGet() {
  const response = await invoke<CommandResponse<VatThresholdStatus>>("vat_threshold_status_get")
  return response.data
}

export async function cashflowOverviewGet() {
  const response = await invoke<CommandResponse<CashflowOverview>>("cashflow_overview_get")
  return response.data
}

export async function yearEndPackageCreate(input: YearEndPackageCreateInput) {
  const response = await invoke<CommandResponse<YearEndPackageSummary>>(
    "year_end_package_create",
    { input },
  )
  return response.data
}

export async function yearEndPackageGet(input: YearEndPackageGetInput) {
  const response = await invoke<CommandResponse<YearEndPackageSummary>>("year_end_package_get", {
    input,
  })
  return response.data
}

export async function yearEndPackageFindByFiscalYear(input: YearEndPackageFindInput) {
  const response = await invoke<CommandResponse<YearEndPackageSummary | null>>(
    "year_end_package_find_by_fiscal_year",
    { input },
  )
  return response.data
}

export async function yearEndPackageApprove(input: YearEndPackageApproveInput) {
  const response = await invoke<CommandResponse<YearEndPackageSummary>>(
    "year_end_package_approve",
    { input },
  )
  return response.data
}

export async function yearEndReadinessGet(input: YearEndReadinessInput) {
  const response = await invoke<CommandResponse<YearEndReadiness>>("year_end_readiness_get", {
    input,
  })
  return response.data
}

export async function yearEndPackageExport(input: YearEndPackageExportInput) {
  const response = await invoke<CommandResponse<YearEndPackageSummary>>(
    "year_end_package_export",
    { input },
  )
  return response.data
}

export async function workspaceSettingsGet() {
  const response = await invoke<CommandResponse<WorkspaceSettings>>("workspace_settings_get")
  return response.data
}

export async function workspaceSettingsSave(input: WorkspaceSettingsSaveInput) {
  const response = await invoke<CommandResponse<WorkspaceSettings>>("workspace_settings_save", {
    input,
  })
  return response.data
}

export async function dashboardTourMarkComplete() {
  const response = await invoke<CommandResponse<WorkspaceSettings>>("dashboard_tour_mark_complete")
  return response.data
}

export async function sieExportCreate(input: SieExportCreateInput) {
  const response = await invoke<CommandResponse<SieExportSummary>>("sie_export_create", { input })
  return response.data
}

export async function accountantPackageExportCreate(input: AccountantPackageExportCreateInput) {
  const response = await invoke<CommandResponse<AccountantPackageExportSummary>>(
    "accountant_package_export_create",
    { input },
  )
  return response.data
}

export async function accountantPackageImportValidate(input: AccountantPackageImportValidateInput) {
  const response = await invoke<CommandResponse<AccountantPackageValidateSummary>>(
    "accountant_package_import_validate",
    { input },
  )
  return response.data
}

export async function integrationStatusGet() {
  const response = await invoke<CommandResponse<IntegrationStatusResponse>>("integration_status_get")
  return response.data
}

export async function voucherList(input: VoucherListInput) {
  const response = await invoke<CommandResponse<VoucherSummary[]>>("voucher_list", { input })
  return response.data
}

export async function voucherCount(input: VoucherCountInput) {
  const response = await invoke<CommandResponse<number>>("voucher_count", { input })
  return response.data
}

export async function voucherGet(input: VoucherGetInput) {
  const response = await invoke<CommandResponse<VoucherDetail>>("voucher_get", { input })
  return response.data
}

export async function accountList() {
  const response = await invoke<CommandResponse<AccountSummary[]>>("account_list")
  return response.data
}

export async function fiscalPeriodList() {
  const response = await invoke<CommandResponse<FiscalPeriodSummary[]>>("fiscal_period_list")
  return response.data
}

export async function stagedTransactionsList(input: StagedTransactionsListInput) {
  const response = await invoke<CommandResponse<StagedTransactionSummary[]>>(
    "staged_transactions_list",
    { input },
  )
  return response.data
}

export async function documentList(input: DocumentListInput) {
  const response = await invoke<CommandResponse<Document[]>>("document_list", { input })
  return response.data
}

export async function documentReveal(input: DocumentGetInput) {
  const response = await invoke<CommandResponse<boolean>>("document_reveal", { input })
  return response.data
}

export function appErrorMessage(error: unknown, fallback: string) {
  if (error && typeof error === "object") {
    const appError = error as { code?: string; message?: string }
    if (appError.code === "storage_error") {
      return fallback
    }
    if ("message" in appError && typeof appError.message === "string" && appError.message.length > 0) {
      return appError.message
    }
  }
  return fallback
}
