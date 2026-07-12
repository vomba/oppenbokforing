use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

use crate::{
    audit::record_event,
    documents,
    error::AppError,
    invoicing::{self, InvoiceSummary},
    profiles,
};

pub const JOB_INVOICE_PDF: &str = "invoice_pdf_generate";
const MAX_PDF_JOB_ATTEMPTS: i64 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvoicePdfJobPayload {
    invoice_id: String,
    invoice_number: String,
    format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InvoicePdfJobResult {
    document_id: String,
    content_sha256: String,
}

struct ClaimedJob {
    id: String,
    payload_json: String,
    attempts: i64,
}

enum ProcessOneOutcome {
    Processed,
    Requeued(String),
    Idle,
}

pub async fn recover_stale_invoice_pdf_jobs(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<u32, AppError> {
    let result = sqlx::query(
        r#"
        UPDATE local_jobs
        SET status = 'queued',
            updated_at = CURRENT_TIMESTAMP
        WHERE workspace_id = ?1
          AND job_type = ?2
          AND status = 'running'
        "#,
    )
    .bind(workspace_id)
    .bind(JOB_INVOICE_PDF)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() as u32)
}

pub async fn process_pending_invoice_pdf_jobs(
    pool: &SqlitePool,
    workspace_id: &str,
) -> Result<u32, AppError> {
    let mut processed = 0u32;
    let mut deferred = HashSet::new();
    loop {
        match process_one_invoice_pdf_job(pool, workspace_id, &deferred).await? {
            ProcessOneOutcome::Processed => processed += 1,
            ProcessOneOutcome::Requeued(job_id) => {
                processed += 1;
                deferred.insert(job_id);
            }
            ProcessOneOutcome::Idle => break,
        }
    }
    Ok(processed)
}

async fn process_one_invoice_pdf_job(
    pool: &SqlitePool,
    workspace_id: &str,
    deferred_job_ids: &HashSet<String>,
) -> Result<ProcessOneOutcome, AppError> {
    let Some(job) = claim_next_job(pool, workspace_id, deferred_job_ids).await? else {
        return Ok(ProcessOneOutcome::Idle);
    };

    let result = run_invoice_pdf_job(pool, workspace_id, &job).await;
    match result {
        Ok(summary) => {
            mark_job_succeeded(pool, &job.id, &summary).await?;
            Ok(ProcessOneOutcome::Processed)
        }
        Err(error) => {
            let requeued = job.attempts < MAX_PDF_JOB_ATTEMPTS;
            mark_job_failed(pool, &job, &error).await?;
            if requeued {
                Ok(ProcessOneOutcome::Requeued(job.id))
            } else {
                Ok(ProcessOneOutcome::Processed)
            }
        }
    }
}

async fn claim_next_job(
    pool: &SqlitePool,
    workspace_id: &str,
    deferred_job_ids: &HashSet<String>,
) -> Result<Option<ClaimedJob>, AppError> {
    let row = if deferred_job_ids.is_empty() {
        sqlx::query(
            r#"
            UPDATE local_jobs
            SET status = 'running',
                attempts = attempts + 1,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = (
              SELECT id FROM local_jobs
              WHERE workspace_id = ?1
                AND job_type = ?2
                AND status = 'queued'
              ORDER BY created_at ASC
              LIMIT 1
            )
            RETURNING id, payload_json, attempts
            "#,
        )
        .bind(workspace_id)
        .bind(JOB_INVOICE_PDF)
        .fetch_optional(pool)
        .await?
    } else {
        let placeholders = deferred_job_ids
            .iter()
            .enumerate()
            .map(|(index, _)| format!("?{}", index + 3))
            .collect::<Vec<_>>()
            .join(", ");
        let query = format!(
            r#"
            UPDATE local_jobs
            SET status = 'running',
                attempts = attempts + 1,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = (
              SELECT id FROM local_jobs
              WHERE workspace_id = ?1
                AND job_type = ?2
                AND status = 'queued'
                AND id NOT IN ({placeholders})
              ORDER BY created_at ASC
              LIMIT 1
            )
            RETURNING id, payload_json, attempts
            "#
        );
        let mut query_builder = sqlx::query(&query)
            .bind(workspace_id)
            .bind(JOB_INVOICE_PDF);
        for job_id in deferred_job_ids {
            query_builder = query_builder.bind(job_id);
        }
        query_builder.fetch_optional(pool).await?
    };

    Ok(row.map(|row| ClaimedJob {
        id: row.get("id"),
        payload_json: row.get("payload_json"),
        attempts: row.get("attempts"),
    }))
}

async fn run_invoice_pdf_job(
    pool: &SqlitePool,
    workspace_id: &str,
    job: &ClaimedJob,
) -> Result<InvoicePdfJobResult, AppError> {
    let payload: InvoicePdfJobPayload = serde_json::from_str(&job.payload_json)
        .map_err(|_| AppError::validation("Invalid invoice PDF job payload", "job"))?;

    if payload.format != "pdf" {
        return Err(AppError::validation("Unsupported PDF job format", "job"));
    }

    let invoice = invoicing::get_invoice(pool, workspace_id, &payload.invoice_id).await?;
    if invoice.status != "issued" {
        return Err(AppError::validation(
            "Invoice must be issued before PDF generation",
            "invoiceId",
        ));
    }

    if let Some(document_id) = existing_pdf_document_id(pool, workspace_id, &invoice.id).await? {
        if document_id.trim().is_empty() {
            // Continue with fresh PDF generation when no document is linked yet.
        } else if let Some(content_sha256) = sqlx::query_scalar(
            r#"
            SELECT content_sha256 FROM documents
            WHERE workspace_id = ?1 AND id = ?2
            LIMIT 1
            "#,
        )
        .bind(workspace_id)
        .bind(&document_id)
        .fetch_optional(pool)
        .await?
        {
            return Ok(InvoicePdfJobResult {
                document_id,
                content_sha256,
            });
        }
    }

    let business = profiles::get_business_profile(pool, workspace_id)
        .await?
        .ok_or_else(|| AppError::validation("Business profile not found", "businessProfile"))?;
    let tax = profiles::get_tax_profile(pool, workspace_id).await?;
    let vat = profiles::get_vat_profile(pool, workspace_id).await?;
    let pdf_context = invoicing::pdf::InvoicePdfContext {
        business_name: business.business_name,
        owner_name: business.owner_name,
        tax_status: tax.map(|profile| profile.tax_status).unwrap_or_default(),
        vat_status: vat.map(|profile| profile.vat_status).unwrap_or_default(),
    };
    let pdf_bytes = tokio::task::spawn_blocking({
        let invoice = invoice.clone();
        move || invoicing::pdf::render_invoice_pdf(&invoice, &pdf_context)
    })
    .await
    .map_err(|error| AppError::internal(error.to_string()))??;
    let filename = format!(
        "invoice-{}.pdf",
        invoice
            .invoice_number
            .as_deref()
            .unwrap_or(&invoice.id)
    );
    let document = documents::store_document_bytes(
        pool,
        workspace_id,
        &pdf_bytes,
        &filename,
        "application/pdf",
    )
    .await?;

    sqlx::query(
        r#"
        UPDATE invoices
        SET pdf_document_id = ?1, updated_at = CURRENT_TIMESTAMP
        WHERE workspace_id = ?2 AND id = ?3
        "#,
    )
    .bind(&document.id)
    .bind(workspace_id)
    .bind(&invoice.id)
    .execute(pool)
    .await?;

    record_event(
        pool,
        workspace_id,
        "invoice_pdf_generated",
        "invoice",
        Some(&invoice.id),
        &serde_json::json!({
            "jobId": job.id,
            "documentId": document.id,
            "invoiceNumber": payload.invoice_number,
        })
        .to_string(),
    )
    .await?;

    Ok(InvoicePdfJobResult {
        document_id: document.id,
        content_sha256: document.content_sha256,
    })
}

async fn existing_pdf_document_id(
    pool: &SqlitePool,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<Option<String>, AppError> {
    sqlx::query_scalar(
        r#"
        SELECT pdf_document_id FROM invoices
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(invoice_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| AppError::internal(error.to_string()))
}

async fn mark_job_succeeded(
    pool: &SqlitePool,
    job_id: &str,
    result: &InvoicePdfJobResult,
) -> Result<(), AppError> {
    let payload = serde_json::to_string(result).map_err(|e| AppError::internal(e.to_string()))?;
    sqlx::query(
        r#"
        UPDATE local_jobs
        SET status = 'succeeded',
            payload_json = ?2,
            last_error = NULL,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?1
        "#,
    )
    .bind(job_id)
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(())
}

async fn mark_job_failed(pool: &SqlitePool, job: &ClaimedJob, error: &AppError) -> Result<(), AppError> {
    let next_status = if job.attempts < MAX_PDF_JOB_ATTEMPTS {
        "queued"
    } else {
        "failed"
    };
    sqlx::query(
        r#"
        UPDATE local_jobs
        SET status = ?2,
            last_error = ?3,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = ?1
        "#,
    )
    .bind(&job.id)
    .bind(next_status)
    .bind(sanitize_job_error(error))
    .execute(pool)
    .await?;
    Ok(())
}

fn sanitize_job_error(error: &AppError) -> String {
    if error.code == "validation_error" {
        error.message.clone()
    } else {
        "invoice_pdf_generation_failed".to_string()
    }
}

pub async fn refresh_invoice_pdf(
    pool: &SqlitePool,
    workspace_id: &str,
    invoice_id: &str,
) -> Result<(), AppError> {
    let invoice = invoicing::get_invoice(pool, workspace_id, invoice_id).await?;
    if invoice.status != "issued" {
        return Err(AppError::validation(
            "Only issued invoices can refresh PDF",
            "invoiceId",
        ));
    }

    sqlx::query(
        r#"
        UPDATE invoices
        SET pdf_document_id = NULL, updated_at = CURRENT_TIMESTAMP
        WHERE workspace_id = ?1 AND id = ?2
        "#,
    )
    .bind(workspace_id)
    .bind(invoice_id)
    .execute(pool)
    .await?;

    let job_id = uuid::Uuid::new_v4().to_string();
    let invoice_number = invoice
        .invoice_number
        .as_deref()
        .unwrap_or(&invoice.id);
    let payload = serde_json::json!({
        "invoiceId": invoice_id,
        "invoiceNumber": invoice_number,
        "format": "pdf"
    });
    sqlx::query(
        r#"
        INSERT INTO local_jobs (id, workspace_id, job_type, status, payload_json)
        VALUES (?1, ?2, 'invoice_pdf_generate', 'queued', ?3)
        "#,
    )
    .bind(&job_id)
    .bind(workspace_id)
    .bind(payload.to_string())
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        UPDATE invoices
        SET pdf_job_id = ?1, updated_at = CURRENT_TIMESTAMP
        WHERE workspace_id = ?2 AND id = ?3
        "#,
    )
    .bind(&job_id)
    .bind(workspace_id)
    .bind(invoice_id)
    .execute(pool)
    .await?;

    process_pending_invoice_pdf_jobs(pool, workspace_id).await?;
    Ok(())
}

pub async fn invoice_pdf_status(
    pool: &SqlitePool,
    workspace_id: &str,
    invoice: &InvoiceSummary,
) -> Result<String, AppError> {
    if let Some(document_id) = invoice.pdf_document_id.as_deref() {
        if !document_id.trim().is_empty() {
            if pdf_document_exists(pool, workspace_id, document_id).await? {
                return Ok("succeeded".to_string());
            }
            return Ok("queued".to_string());
        }
    }
    let Some(job_id) = &invoice.pdf_job_id else {
        return Ok("none".to_string());
    };
    let status: Option<String> = sqlx::query_scalar(
        r#"
        SELECT status FROM local_jobs
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(job_id)
    .fetch_optional(pool)
    .await?;
    Ok(status.unwrap_or_else(|| "unknown".to_string()))
}

async fn pdf_document_exists(
    pool: &SqlitePool,
    workspace_id: &str,
    document_id: &str,
) -> Result<bool, AppError> {
    let exists: Option<String> = sqlx::query_scalar(
        r#"
        SELECT id FROM documents
        WHERE workspace_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(workspace_id)
    .bind(document_id)
    .fetch_optional(pool)
    .await?;
    Ok(exists.is_some())
}
