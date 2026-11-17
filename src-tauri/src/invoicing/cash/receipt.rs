//! Receipt-only posting boundary. Owner facts are declarations, not OCR or bank authentication.
//! Retained integrity is verified at posting; durable history/replay never claims fresh verification.
use std::{
    io::Read,
    path::{Component, Path},
};

use chrono::Utc;
use uuid::Uuid;

use super::*;
use crate::invoicing::{eligibility, InvoiceIssueReviewSnapshot};

const JOB: &str = "invoice_cash_receipt_record";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompletedReceipt {
    idempotency_key: String,
    request: Value,
    receipt_id: String,
    result: ReconciliationMatchResult,
}
const RECEIPT_COVERAGE: &str = "Full same-2026 SEK cash receipt: retained external PDF integrity verified AT POSTING plus explicit owner receipt facts; not fresh evidence verification on history/replay, bank authentication, OCR or named-bank validation. One receipt per PDF hash and bank tuple. No partial, credit, cash VAT/year-end approval or unpaid closing coverage.";

fn request(input: &CashReceiptRecordInput) -> Value {
    json!({"invoiceId":input.invoice_id,"documentId":input.document_id,"facts":input.facts})
}

// No guessed bank formatting: preserve punctuation. ASCII-only, unspaced
// identifiers have deterministic trim/case normalization. Reject internal whitespace,
// unsupported Unicode, controls and placeholders rather than guessing equivalence.
fn identity_part(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty()
        || trimmed.len() > 256
        || !trimmed.is_ascii()
        || trimmed
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace())
        || matches!(
            trimmed.to_ascii_lowercase().as_str(),
            "unknown" | "n/a" | "?" | "none"
        )
    {
        return Err(blocked(
            "Explicit unambiguous bank, receiving account and transaction reference required",
        ));
    }
    Ok(trimmed.to_ascii_uppercase())
}
fn bank_identity(facts: &CashReceiptFacts) -> Result<String, AppError> {
    let bank = facts
        .bank_receipt_identity
        .as_ref()
        .ok_or_else(|| blocked("Bank receipt identity required"))?;
    encode(&[
        identity_part(&bank.bank_id)?,
        identity_part(&bank.receiving_account_reference)?,
        identity_part(&bank.transaction_reference)?,
    ])
}
fn validate_facts(
    facts: &CashReceiptFacts,
    gross: i64,
    issued_on: &str,
) -> Result<String, AppError> {
    if gross <= 0
        || facts.actual_received_amount_minor != Some(gross)
        || facts.currency.as_deref() != Some("SEK")
        || facts.received_already != Some(true)
        || facts.bank_pdf_matches_receipt_confirmed != Some(true)
        || facts.issued_case_still_applies_confirmed != Some(true)
        || facts.no_other_receipts_or_adjustments_confirmed != Some(true)
        || !facts.source_entry_locator.as_deref().is_some_and(|s| {
            !s.trim().is_empty() && s.len() <= 1024 && !s.chars().any(char::is_control)
        })
    {
        return Err(blocked("Nine explicit actual receipt facts, exact positive full original amount, SEK and confirmed received/matching/unchanged/sole receipt are required"));
    }
    let received = facts
        .actual_received_on
        .as_deref()
        .ok_or_else(|| blocked("Actual received date required"))?;
    if policy::date(received)? < policy::date(issued_on)? {
        return Err(blocked("Actual receipt cannot precede issue"));
    }
    bank_identity(facts)
}

pub(super) async fn validate_issue_snapshot_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace: &str,
    invoice_id: &str,
    context: &Value,
) -> Result<InvoiceSummary, AppError> {
    // Validate raw integers before the legacy summary's display arithmetic. That
    // reader predates checked cash amounts and cannot be our validation boundary.
    let raw_lines: Vec<(i64,i64,i64,String)> = sqlx::query_as("SELECT quantity,unit_price_minor,vat_rate_bp,account_number FROM invoice_lines WHERE invoice_id=?1").bind(invoice_id).fetch_all(&mut **tx).await?;
    let mut checked = Vec::new();
    for (quantity, unit, rate, account) in raw_lines {
        if rate != 2500 || account != "3041" {
            return Err(blocked("Cash line outside exact service case"));
        }
        let (net, vat, _) = eligibility::exact_amounts(quantity, unit)?;
        checked.push((net, vat));
    }
    let (raw_net, raw_vat, raw_gross) = eligibility::checked_totals(checked)?;
    let reviewed = &context["reviewedInvoice"];
    if raw_net > 100_000_000
        || reviewed["totalExVatMinor"].as_i64() != Some(raw_net)
        || reviewed["totalVatMinor"].as_i64() != Some(raw_vat)
        || reviewed["totalIncVatMinor"].as_i64() != Some(raw_gross)
    {
        return Err(blocked(
            "Actual cash lines differ from immutable reviewed amounts",
        ));
    }
    let invoice = super::super::fetch_invoice_summary_tx(tx, workspace, invoice_id).await?;
    if invoice.status != "issued"
        || invoice.invoice_kind != "standard"
        || invoice.source_invoice_id.is_some()
        || invoice.voucher_id.is_some()
        || invoice.issue_date.as_deref() != context["issueDate"].as_str()
    {
        return Err(blocked(
            "Original cash issue must remain standard, issued and uncredited with no issue voucher",
        ));
    }
    let credited: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM invoices WHERE workspace_id=?1 AND source_invoice_id=?2) OR EXISTS(SELECT 1 FROM vouchers WHERE workspace_id=?1 AND source_type='invoice' AND source_id=?2)")
        .bind(workspace).bind(invoice_id).fetch_one(&mut **tx).await?;
    if credited || invoice.lines.is_empty() {
        return Err(blocked(
            "Cash issue has incompatible credit/posting history",
        ));
    }
    for line in &invoice.lines {
        let (net, vat, _) = eligibility::exact_amounts(line.quantity, line.unit_price_minor)?;
        if line.account_number != "3041"
            || line.vat_rate_bp != 2500
            || line.line_ex_vat_minor != net
            || line.line_vat_minor != vat
        {
            return Err(blocked(
                "Original cash line differs from exact service case",
            ));
        }
    }
    let (net, vat, gross) = eligibility::checked_totals(
        invoice
            .lines
            .iter()
            .map(|l| (l.line_ex_vat_minor, l.line_vat_minor)),
    )?;
    if (net, vat, gross)
        != (
            invoice.total_ex_vat_minor,
            invoice.total_vat_minor,
            invoice.total_inc_vat_minor,
        )
    {
        return Err(blocked("Original cash header differs from checked lines"));
    }
    let reviewed = InvoiceIssueReviewSnapshot {
        invoice_id: invoice.id.clone(),
        counterparty_id: invoice.counterparty_id.clone(),
        counterparty_name: invoice.counterparty_name.clone(),
        due_date: invoice.due_date.clone(),
        total_ex_vat_minor: net,
        total_vat_minor: vat,
        total_inc_vat_minor: gross,
        lines: invoice.lines.clone(),
    };
    let snapshot = sqlx::query("SELECT business_name,owner_name,tax_status,vat_status,rule_version_id,tax_year,source_url FROM invoice_issue_snapshots WHERE workspace_id=?1 AND invoice_id=?2")
        .bind(workspace).bind(invoice_id).fetch_optional(&mut **tx).await?.ok_or_else(|| blocked("Immutable issue snapshot missing"))?;
    let snapshot_value = json!({"businessName":snapshot.try_get::<String,_>("business_name")?,"ownerName":snapshot.try_get::<String,_>("owner_name")?,"taxStatus":snapshot.try_get::<String,_>("tax_status")?,"vatStatus":snapshot.try_get::<String,_>("vat_status")?,"ruleVersionId":snapshot.try_get::<String,_>("rule_version_id")?,"taxYear":snapshot.try_get::<i32,_>("tax_year")?,"sourceUrl":snapshot.try_get::<String,_>("source_url")?});
    let expected_snapshot = json!({"businessName":context["business"]["businessName"],"ownerName":context["business"]["ownerName"],"taxStatus":context["tax"]["taxStatus"],"vatStatus":context["vat"]["vatStatus"],"ruleVersionId":CASH_V1.rule_id,"taxYear":2026,"sourceUrl":context["rules"]["sourceUrl"]});
    let issued_row = sqlx::query("SELECT result_json,net_minor,request_json FROM cash_invoice_issues WHERE workspace_id=?1 AND invoice_id=?2").bind(workspace).bind(invoice_id).fetch_one(&mut **tx).await?;
    let issued: InvoiceSummary = decode(&issued_row.try_get::<String, _>("result_json")?)?;
    let issue_request: Value = decode(&issued_row.try_get::<String, _>("request_json")?)?;
    if context["reviewedInvoice"] != value(&reviewed)?
        || snapshot_value != expected_snapshot
        || issued.invoice_number != invoice.invoice_number
        || issued.issue_date != invoice.issue_date
        || issued.total_ex_vat_minor != net
        || issued.total_vat_minor != vat
        || issued.total_inc_vat_minor != gross
        || issued_row.try_get::<i64, _>("net_minor")? != net
        || issue_request["invoiceId"] != invoice_id
        || issue_request["facts"] != context["facts"]
        || issue_request["issueDate"] != context["issueDate"]
        || issue_request["reviewFingerprint"] != hash(encode(context)?.as_bytes())
    {
        return Err(blocked(
            "Immutable cash issue snapshot/request/identity is inconsistent",
        ));
    }
    Ok(invoice)
}

fn receipt_assessment(context: &Value) -> Result<CashReceiptAssessment, AppError> {
    Ok(CashReceiptAssessment {
        validation_id: CASH_V1.id.into(),
        evidence_digest: CASH_V1.digest(),
        review_fingerprint: hash(encode(context)?.as_bytes()),
        rule_version_id: CASH_V1.rule_id.into(),
        origin_id: context["originId"]
            .as_str()
            .ok_or_else(|| blocked("Receipt origin missing"))?
            .into(),
        accounting_year: 2026,
        accounting_date: context["request"]["facts"]["actualReceivedOn"]
            .as_str()
            .ok_or_else(|| blocked("Receipt date missing"))?
            .into(),
        period_key: "2026".into(),
        due_on: "2027-05-12".into(),
        coverage_summary: RECEIPT_COVERAGE.into(),
        document_id: context["request"]["documentId"]
            .as_str()
            .ok_or_else(|| blocked("Receipt document missing"))?
            .into(),
        document_sha256: context["documentSha256"]
            .as_str()
            .ok_or_else(|| blocked("Receipt hash missing"))?
            .into(),
    })
}

async fn source_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace: &str,
    document_id: &str,
) -> Result<(crate::documents::Document, Vec<u8>), AppError> {
    // Refuse oversized retained files before the shared verifier reads them. This
    // metadata check grants no integrity/path permission; the archive verifier
    // below still owns workspace association, symlinks, path containment and hash.
    let source: Option<(String,String)> = sqlx::query_as("SELECT w.documents_path,d.object_path FROM documents d JOIN workspaces w ON w.id=d.workspace_id WHERE d.workspace_id=?1 AND d.id=?2").bind(workspace).bind(document_id).fetch_optional(&mut **tx).await?;
    let (root, relative) =
        source.ok_or_else(|| blocked("Retained document missing from workspace"))?;
    let relative = Path::new(&relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(blocked("Invalid retained PDF path"));
    }
    let length = std::fs::symlink_metadata(Path::new(&root).join(relative))
        .map_err(|_| blocked("Retained PDF unavailable"))?
        .len();
    if length > 10 * 1024 * 1024 {
        return Err(blocked("Retained PDF exceeds 10 MiB boundary"));
    }
    let document =
        crate::documents::verify_retained_document_tx(tx, workspace, document_id).await?;
    // Include all completed PDF jobs (old refresh results too), and legacy preserved
    // hashes, not only today's invoice.pdf_document_id.
    let generated: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM invoices i JOIN documents d ON d.id=i.pdf_document_id WHERE i.workspace_id=?1 AND d.content_sha256=?2) OR EXISTS(SELECT 1 FROM local_jobs WHERE workspace_id=?1 AND job_type='invoice_pdf_generate' AND (json_extract(payload_json,'$.contentSha256')=?2 OR json_extract(payload_json,'$.documentId')=?3)) OR EXISTS(SELECT 1 FROM invoice_snapshot_recovery_requirements WHERE workspace_id=?1 AND (preserved_pdf_content_sha256=?2 OR preserved_pdf_document_id=?3))")
        .bind(workspace).bind(&document.content_sha256).bind(document_id).fetch_one(&mut **tx).await?;
    if generated || !crate::documents::is_pdf_mime(&document.mime_type) {
        return Err(blocked(
            "Source must be an external retained bank PDF, never a generated invoice",
        ));
    }
    let root: String = sqlx::query_scalar("SELECT documents_path FROM workspaces WHERE id=?1")
        .bind(workspace)
        .fetch_one(&mut **tx)
        .await?;
    let relative = Path::new(&document.object_path);
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(blocked("Invalid retained path"));
    }
    let root = std::fs::canonicalize(root).map_err(|_| blocked("Retained PDF unavailable"))?;
    let path = root.join(relative);
    let canonical =
        std::fs::canonicalize(&path).map_err(|_| blocked("Retained PDF unavailable"))?;
    if !canonical.starts_with(&root) {
        return Err(blocked("Retained PDF outside workspace"));
    }
    // Bounded reread with the same hash binds the parsed bytes to verified evidence.
    // No rendering, external resource loading or bank-text extraction is used.
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options
        .open(path)
        .map_err(|_| blocked("Retained PDF unavailable"))?;
    if !file
        .metadata()
        .map_err(|_| blocked("PDF metadata unavailable"))?
        .is_file()
    {
        return Err(blocked("PDF must be a regular file"));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(10 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| blocked("Retained PDF unavailable"))?;
    if bytes.len() > 10 * 1024 * 1024
        || !bytes.starts_with(b"%PDF-")
        || hash(&bytes) != document.content_sha256
    {
        return Err(blocked("Retained PDF bytes invalid or changed"));
    }
    Ok((document, bytes))
}

// Historical proof verification intentionally never checks fresh files, helper
// presence, live rule activation or current source availability.
async fn completed_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace: &str,
    input: &CashReceiptRecordInput,
    key: &str,
) -> Result<Option<ReconciliationMatchResult>, AppError> {
    let replay = sqlx::query("SELECT invoice_id,request_json FROM cash_receipt_recognitions WHERE workspace_id=?1 AND idempotency_key=?2").bind(workspace).bind(key).fetch_optional(&mut **tx).await?;
    if let Some(row) = replay {
        if row.try_get::<String, _>("invoice_id")? != input.invoice_id
            || decode::<Value>(&row.try_get::<String, _>("request_json")?)? != request(input)
        {
            return Err(blocked("Completed cash receipt replay input differs"));
        }
        let history = history_tx(tx, workspace, &input.invoice_id)
            .await?
            .ok_or_else(|| blocked("Receipt issue missing"))?;
        let receipt = history
            .receipt
            .ok_or_else(|| blocked("Receipt proof missing"))?;
        return Ok(Some(receipt.result));
    }
    Ok(None)
}

pub(super) async fn record(
    pool: &SqlitePool,
    workspace: &str,
    input: &CashReceiptRecordInput,
) -> Result<ReconciliationMatchResult, AppError> {
    let key = crate::idempotency::normalize_idempotency_key(&input.idempotency_key)?;
    // Fast read-only recovery precedes all fresh source/helper/current guards.
    let mut read = pool.begin().await?;
    if let Some(result) = completed_tx(&mut read, workspace, input, key).await? {
        read.commit().await?;
        return Ok(result);
    }
    let history = history_tx(&mut read, workspace, &input.invoice_id)
        .await?
        .ok_or_else(|| blocked("Original immutable cash issue required"))?;
    if history.receipt.is_some() {
        return Err(blocked(
            "Invoice already has a cash receipt; new key is not replay",
        ));
    }
    read.commit().await?;

    // Hold a bounded slot before any source allocation/connection, release every
    // read transaction/connection BEFORE exec. Only the killable child parses.
    let prepared = async {
        let permit = super::pdf_process::slot().await?;
        let mut read = pool.begin().await?;
        let (document, bytes) = source_tx(&mut read, workspace, &input.document_id).await?;
        read.commit().await?;
        super::pdf_process::inspect(workspace, &document, bytes, permit).await
    }
    .await;

    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    // Concurrent completion wins even if preparation just failed (file/helper
    // disappeared). Recheck exact raw request after acquiring the writer.
    if let Some(result) = completed_tx(&mut tx, workspace, input, key).await? {
        tx.commit().await?;
        return Ok(result);
    }
    let prepared = prepared?;
    let collision: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM local_jobs WHERE workspace_id=?1 AND idempotency_key=?2)",
    )
    .bind(workspace)
    .bind(key)
    .fetch_one(&mut *tx)
    .await?;
    if collision {
        return Err(blocked("Idempotency key belongs to another operation"));
    }
    let history = history_tx(&mut tx, workspace, &input.invoice_id)
        .await?
        .ok_or_else(|| blocked("Original immutable cash issue required"))?;
    if history.receipt.is_some() {
        return Err(blocked(
            "Invoice already has a cash receipt; new key is not replay",
        ));
    }
    let old_context: String = sqlx::query_scalar(
        "SELECT context_json FROM cash_invoice_issues WHERE workspace_id=?1 AND invoice_id=?2",
    )
    .bind(workspace)
    .bind(&input.invoice_id)
    .fetch_one(&mut *tx)
    .await?;
    let old_context: Value = decode(&old_context)?;
    let invoice =
        validate_issue_snapshot_tx(&mut tx, workspace, &input.invoice_id, &old_context).await?;
    if invoice.payment_voucher_id.is_some() {
        return Err(blocked("Cash invoice already paid"));
    }
    let bank = validate_facts(
        &input.facts,
        invoice.total_inc_vat_minor,
        &history.issue.assessment.issued_on,
    )?;
    let received = input.facts.actual_received_on.as_deref().unwrap();
    policy::ensure_not_future(received, Utc::now())?;
    let evaluation = policy::evaluate_stage_tx(
        &mut tx,
        workspace,
        &CashIssuePreflightInput {
            invoice_id: input.invoice_id.clone(),
            issue_date: history.issue.assessment.issued_on.clone(),
            facts: history.issue.facts.clone(),
            origin_review: None,
        },
        Some(received),
    )
    .await?;
    // Annual commitments may legitimately change. All original relevant posture,
    // actual rules, reviewed lines and fiscal identity must still match.
    for field in [
        "business",
        "tax",
        "vat",
        "rules",
        "reviewedInvoice",
        "fiscalYearId",
        "yearEnd",
        "originId",
        "originFacts",
        "firstInvoiceId",
    ] {
        if evaluation.context[field] != old_context[field] {
            return Err(blocked(format!(
                "Cash issue context no longer applies: {field}"
            )));
        }
    }
    let origin_year: String = sqlx::query_scalar(
        "SELECT fiscal_year_id FROM cash_business_origins WHERE workspace_id=?1 AND id=?2",
    )
    .bind(workspace)
    .bind(&history.origin.id)
    .fetch_one(&mut *tx)
    .await?;
    if origin_year != evaluation.fiscal_year_id {
        return Err(blocked("Receipt fiscal year differs from immutable origin"));
    }
    let approved: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM year_end_packages WHERE workspace_id=?1 AND fiscal_year_id=?2 AND status='approved') OR EXISTS(SELECT 1 FROM vat_returns r JOIN fiscal_periods p ON p.workspace_id=r.workspace_id AND p.id=r.fiscal_period_id WHERE r.workspace_id=?1 AND p.fiscal_year_id=?2 AND r.status='approved' AND p.starts_on<=?3 AND p.ends_on>=?3)").bind(workspace).bind(&evaluation.fiscal_year_id).bind(received).fetch_one(&mut *tx).await?;
    if approved {
        return Err(blocked(
            "Receipt accounting year/period is already approved",
        ));
    }
    // Reverify SAME retained association/hash and generated-invoice exclusion in
    // writer. This bounded byte verification performs NO in-process PDF parsing.
    let (document, _) = source_tx(&mut tx, workspace, &input.document_id).await?;
    prepared.matches(workspace, &document)?;
    let duplicate: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cash_receipt_recognitions WHERE workspace_id=?1 AND (invoice_id=?2 OR document_sha256=?3 OR bank_identity=?4))").bind(workspace).bind(&input.invoice_id).bind(&document.content_sha256).bind(&bank).fetch_one(&mut *tx).await?;
    if duplicate {
        return Err(blocked(
            "Receipt invoice, PDF hash or bank tuple already consumed",
        ));
    }
    let receipt_id = Uuid::new_v4().to_string();
    let voucher = Uuid::new_v4().to_string();
    let staged = Uuid::new_v4().to_string();
    let matched = Uuid::new_v4().to_string();
    let result = ReconciliationMatchResult {
        match_id: matched.clone(),
        voucher_id: Some(voucher.clone()),
    };
    let context = json!({"validationId":CASH_V1.id,"evidenceDigest":CASH_V1.digest(),"workspaceId":workspace,"invoiceId":input.invoice_id,"originId":history.origin.id,"issueFingerprint":history.issue.assessment.review_fingerprint,"request":request(input),"documentSha256":document.content_sha256,"bankIdentity":bank,"fiscalYearId":evaluation.fiscal_year_id,"currentTaxableBaseMinor":evaluation.preflight.current_taxable_base_minor,"receiptId":receipt_id,"voucherId":voucher,"matchId":matched,"stagedTransactionId":staged,"evidenceVerification":"retained_pdf_integrity_at_posting_not_bank_authentication","result":result});
    let assessment = receipt_assessment(&context)?;
    // Reserve the immutable proof BEFORE journal writes. Migration 22's deferred
    // workspace-scoped FKs validate all planned links at this transaction's commit.
    sqlx::query("INSERT INTO cash_receipt_recognitions(id,workspace_id,invoice_id,origin_id,document_id,document_sha256,bank_identity,voucher_id,match_id,staged_transaction_id,idempotency_key,facts_json,assessment_json,context_json,request_json,result_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)")
        .bind(&receipt_id).bind(workspace).bind(&input.invoice_id).bind(&history.origin.id).bind(&input.document_id).bind(&document.content_sha256).bind(&bank).bind(&voucher).bind(&matched).bind(&staged).bind(key).bind(encode(&input.facts)?).bind(encode(&assessment)?).bind(encode(&context)?).bind(encode(&request(input))?).bind(encode(&result)?).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO vouchers(id,workspace_id,fiscal_year_id,status,source_type,source_id,posted_at,accounting_date,document_id) VALUES(?1,?2,?3,'posted','reconciliation',?4,CURRENT_TIMESTAMP,?5,?6)").bind(&voucher).bind(workspace).bind(&evaluation.fiscal_year_id).bind(&input.invoice_id).bind(received).bind(&input.document_id).execute(&mut *tx).await?;
    for (number, debit, credit, vat_code) in [
        ("1930", invoice.total_inc_vat_minor, 0, None),
        ("3041", 0, invoice.total_ex_vat_minor, None),
        ("2611", 0, invoice.total_vat_minor, Some("VAT25")),
    ] {
        let account: String =
            sqlx::query_scalar("SELECT id FROM accounts WHERE workspace_id=?1 AND number=?2")
                .bind(workspace)
                .bind(number)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or_else(|| blocked("Required scoped cash account missing"))?;
        sqlx::query("INSERT INTO journal_lines(id,voucher_id,account_id,debit_minor,credit_minor,vat_code) VALUES(?1,?2,?3,?4,?5,?6)").bind(Uuid::new_v4().to_string()).bind(&voucher).bind(account).bind(debit).bind(credit).bind(vat_code).execute(&mut *tx).await?;
    }
    sqlx::query("INSERT INTO staged_transactions(id,workspace_id,csv_import_id,transaction_date,description,amount_minor,status) VALUES(?1,?2,NULL,?3,?4,?5,'matched')").bind(&staged).bind(workspace).bind(received).bind(format!("Retained PDF cash receipt: {} / {}",document.original_filename,input.facts.source_entry_locator.as_deref().unwrap())).bind(input.facts.actual_received_amount_minor.unwrap()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO reconciliation_matches(id,workspace_id,staged_transaction_id,match_kind,invoice_id,voucher_id) VALUES(?1,?2,?3,'invoice_payment',?4,?5)").bind(&matched).bind(workspace).bind(&staged).bind(&input.invoice_id).bind(&voucher).execute(&mut *tx).await?;
    let payload = CompletedReceipt {
        idempotency_key: key.into(),
        request: request(input),
        receipt_id: receipt_id.clone(),
        result: result.clone(),
    };
    sqlx::query("INSERT INTO local_jobs(id,workspace_id,job_type,status,payload_json,idempotency_key) VALUES(?1,?2,?3,'succeeded',?4,?5)").bind(Uuid::new_v4().to_string()).bind(workspace).bind(JOB).bind(encode(&payload)?).bind(key).execute(&mut *tx).await?;
    crate::audit::record_event_tx(
        &mut *tx,
        workspace,
        JOB,
        "cash_receipt_recognition",
        Some(&receipt_id),
        &encode(&context)?,
    )
    .await?;
    history_receipt_tx(
        &mut tx,
        workspace,
        &input.invoice_id,
        &history.origin,
        &history.issue,
        &old_context,
    )
    .await?
    .ok_or_else(|| blocked("Receipt proof missing after posting"))?;
    tx.commit().await?;
    Ok(result)
}

pub(super) async fn history_receipt_tx(
    tx: &mut Transaction<'_, Sqlite>,
    workspace: &str,
    invoice_id: &str,
    origin: &CashBusinessOrigin,
    issue: &CashIssueRecord,
    issue_context: &Value,
) -> Result<Option<CashReceiptRecord>, AppError> {
    let row = sqlx::query(
        "SELECT * FROM cash_receipt_recognitions WHERE workspace_id=?1 AND invoice_id=?2",
    )
    .bind(workspace)
    .bind(invoice_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let facts: CashReceiptFacts = decode(&row.try_get::<String, _>("facts_json")?)?;
    let assessment: CashReceiptAssessment = decode(&row.try_get::<String, _>("assessment_json")?)?;
    let context: Value = decode(&row.try_get::<String, _>("context_json")?)?;
    let stored_request: Value = decode(&row.try_get::<String, _>("request_json")?)?;
    let result: ReconciliationMatchResult = decode(&row.try_get::<String, _>("result_json")?)?;
    let invoice = validate_issue_snapshot_tx(tx, workspace, invoice_id, issue_context).await?;
    let bank = validate_facts(
        &facts,
        invoice.total_inc_vat_minor,
        &issue.assessment.issued_on,
    )?;
    let expected_request =
        json!({"invoiceId":invoice_id,"documentId":assessment.document_id,"facts":facts});
    if value(&assessment)? != value(&receipt_assessment(&context)?)?
        || assessment.origin_id != origin.id
        || stored_request != expected_request
        || context["request"] != stored_request
        || context["workspaceId"] != workspace
        || context["invoiceId"] != invoice_id
        || context["issueFingerprint"] != issue.assessment.review_fingerprint
        || context["validationId"] != CASH_V1.id
        || context["evidenceDigest"] != CASH_V1.digest()
        || context["fiscalYearId"] != issue_context["fiscalYearId"]
        || context["result"] != value(&result)?
        || context["bankIdentity"] != bank
        || context["evidenceVerification"]
            != "retained_pdf_integrity_at_posting_not_bank_authentication"
        || !context["currentTaxableBaseMinor"]
            .as_i64()
            .is_some_and(|n| (0..=100_000_000).contains(&n))
    {
        return Err(blocked(
            "Retained receipt assessment/context/facts are inconsistent",
        ));
    }
    for (column, field) in [
        ("id", "receiptId"),
        ("voucher_id", "voucherId"),
        ("match_id", "matchId"),
        ("staged_transaction_id", "stagedTransactionId"),
        ("origin_id", "originId"),
        ("document_sha256", "documentSha256"),
        ("bank_identity", "bankIdentity"),
    ] {
        if context[field] != row.try_get::<String, _>(column)? {
            return Err(blocked("Receipt proof identity/link mismatch"));
        }
    }
    if row.try_get::<String, _>("document_id")? != assessment.document_id
        || result.match_id != row.try_get::<String, _>("match_id")?
        || result.voucher_id.as_deref() != Some(row.try_get::<String, _>("voucher_id")?.as_str())
    {
        return Err(blocked("Receipt result/source mismatch"));
    }
    let linked: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM reconciliation_matches m JOIN staged_transactions s ON s.workspace_id=m.workspace_id AND s.id=m.staged_transaction_id JOIN vouchers v ON v.workspace_id=m.workspace_id AND v.id=m.voucher_id JOIN documents d ON d.workspace_id=m.workspace_id AND d.id=?7 WHERE m.workspace_id=?1 AND m.id=?2 AND m.invoice_id=?3 AND m.voucher_id=?4 AND m.staged_transaction_id=?5 AND m.match_kind='invoice_payment' AND s.status='matched' AND s.csv_import_id IS NULL AND s.transaction_date=?6 AND s.amount_minor=?8 AND v.status='posted' AND v.source_type='reconciliation' AND v.source_id=?3 AND v.accounting_date=?6 AND v.document_id=?7 AND v.fiscal_year_id=?9 AND d.content_sha256=?10)")
        .bind(workspace).bind(&result.match_id).bind(invoice_id).bind(&result.voucher_id).bind(row.try_get::<String,_>("staged_transaction_id")?).bind(&assessment.accounting_date).bind(&assessment.document_id).bind(invoice.total_inc_vat_minor).bind(issue_context["fiscalYearId"].as_str()).bind(&assessment.document_sha256).fetch_one(&mut **tx).await?;
    if !linked {
        return Err(blocked(
            "Receipt source/match/staging/voucher links inconsistent",
        ));
    }
    let lines: Vec<(String,String,i64,i64,Option<String>)> = sqlx::query_as("SELECT a.workspace_id,a.number,j.debit_minor,j.credit_minor,j.vat_code FROM journal_lines j JOIN accounts a ON a.id=j.account_id WHERE j.voucher_id=?1 ORDER BY a.number").bind(&result.voucher_id).fetch_all(&mut **tx).await?;
    let expected = vec![
        (
            workspace.into(),
            "1930".into(),
            invoice.total_inc_vat_minor,
            0,
            None,
        ),
        (
            workspace.into(),
            "2611".into(),
            0,
            invoice.total_vat_minor,
            Some("VAT25".into()),
        ),
        (
            workspace.into(),
            "3041".into(),
            0,
            invoice.total_ex_vat_minor,
            None,
        ),
    ];
    if lines != expected {
        return Err(blocked(
            "Receipt journals do not match balanced original amounts",
        ));
    }
    let key: String = row.try_get("idempotency_key")?;
    let payload: Option<String> = sqlx::query_scalar("SELECT payload_json FROM local_jobs WHERE workspace_id=?1 AND job_type=?2 AND idempotency_key=?3 AND status='succeeded'").bind(workspace).bind(JOB).bind(&key).fetch_optional(&mut **tx).await?;
    let expected_payload = json!({"idempotencyKey":key,"request":stored_request,"receiptId":row.try_get::<String,_>("id")?,"result":result});
    let completed: CompletedReceipt =
        decode(&payload.ok_or_else(|| blocked("Completed receipt replay missing"))?)?;
    if value(&completed)? != expected_payload {
        return Err(blocked("Completed receipt replay inconsistent"));
    }
    let audits: Vec<String> = sqlx::query_scalar("SELECT metadata_json FROM audit_events WHERE workspace_id=?1 AND action=?2 AND resource_type='cash_receipt_recognition' AND resource_id=?3").bind(workspace).bind(JOB).bind(row.try_get::<String,_>("id")?).fetch_all(&mut **tx).await?;
    if audits.len() != 1 || decode::<Value>(&audits[0])? != context {
        return Err(blocked("Receipt immutable audit link inconsistent"));
    }
    Ok(Some(CashReceiptRecord {
        invoice_id: invoice_id.into(),
        facts,
        assessment,
        result,
        recorded_at: row.try_get("recorded_at")?,
    }))
}
