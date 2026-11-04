//! Test boundaries: reviewed live issuance versus explicit pre-gate history.

use oppenbokforing_desktop_lib::{
    error::AppError,
    invoicing::{
        self, InvoiceEligibilityFacts, InvoiceIssueInput, InvoiceIssuePreflightInput,
        InvoiceSummary,
    },
};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub fn facts(completed_on: &str) -> InvoiceEligibilityFacts {
    InvoiceEligibilityFacts {
        ordinary_domestic_service: Some(true),
        service_completed_on: Some(completed_on.into()),
        prior_payment_received: Some(false),
        no_eu_trade_in_year: Some(true),
        annual_vat_registration_confirmed: Some(true),
        expected_taxable_base_within_limit: Some(true),
        ordinary_invoice_timing_confirmed: Some(true),
    }
}

/// Explicit answers and a fresh final review, not a permit or a production bypass.
/// Existing replay is deliberately delegated before collecting new answers.
pub async fn issue_reviewed(
    pool: &SqlitePool,
    workspace: &str,
    input: &InvoiceIssueInput,
) -> Result<InvoiceSummary, AppError> {
    if invoicing::check_issue_idempotency(pool, workspace, &input.idempotency_key)
        .await?
        .is_some()
        || invoicing::get_invoice(pool, workspace, &input.invoice_id)
            .await
            .is_ok_and(|invoice| invoice.status == "issued")
    {
        return invoicing::issue_invoice(pool, workspace, input).await;
    }
    let reviewed = match reviewed_input(pool, workspace, input).await {
        Ok(reviewed) => reviewed,
        Err(_) => {
            // Negative issue tests must still exercise ISSUE, not pass merely
            // because their setup preflight failed. Supply explicit facts but
            // no review when the current case could not be reviewed.
            let mut rejected = input.clone();
            rejected.eligibility_facts = input.issue_date.as_deref().map(facts);
            rejected
        }
    };
    invoicing::issue_invoice(pool, workspace, &reviewed).await
}

pub async fn reviewed_input(
    pool: &SqlitePool,
    workspace: &str,
    input: &InvoiceIssueInput,
) -> Result<InvoiceIssueInput, AppError> {
    let date = input
        .issue_date
        .as_deref()
        .expect("live tests select sending date explicitly");
    let facts = facts(date);
    let preflight = invoicing::invoice_issue_preflight(
        pool,
        workspace,
        &InvoiceIssuePreflightInput {
            invoice_id: input.invoice_id.clone(),
            issue_date: input.issue_date.clone(),
            eligibility_facts: Some(facts.clone()),
        },
    )
    .await?;
    let assessment = preflight
        .eligibility
        .expect("successful new preflight has an assessment");
    Ok(InvoiceIssueInput {
        invoice_id: input.invoice_id.clone(),
        idempotency_key: input.idempotency_key.clone(),
        issue_date: input.issue_date.clone(),
        eligibility_facts: Some(facts),
        review_fingerprint: Some(assessment.review_fingerprint),
    })
}

/// Engineering history ONLY. Materializes the pre-gate 44f897e row shape with
/// test SQL; does not call issue/post, create validation provenance, alter the
/// profile, or disable any trigger. See fixtures/historical-regressions/README.md.
/// Call only for positive historical setup, never to test authorization.
pub async fn seed_historical_invoice(
    pool: &SqlitePool,
    workspace: &str,
    input: &InvoiceIssueInput,
) -> Result<InvoiceSummary, AppError> {
    assert!(
        input.eligibility_facts.is_none() && input.review_fingerprint.is_none(),
        "historical fixtures must not pretend to be newly reviewed cases"
    );
    let date = input
        .issue_date
        .as_deref()
        .expect("historical fixture date");
    let year: i32 = date[..4].parse().expect("historical fixture year");
    let fy = oppenbokforing_desktop_lib::workspace::fiscal_year_id_for_year(pool, workspace, year)
        .await?;
    let draft = invoicing::get_invoice(pool, workspace, &input.invoice_id).await?;
    assert_eq!(draft.status, "draft", "history seeding is not a replay API");
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let sequence = sqlx::query("SELECT id, prefix, next_number FROM invoice_sequences WHERE workspace_id=?1 AND fiscal_year_id=?2")
        .bind(workspace).bind(&fy).fetch_one(&mut *tx).await?;
    let number = format!(
        "{}{:04}",
        sequence.get::<String, _>("prefix"),
        sequence.get::<i64, _>("next_number")
    );
    sqlx::query("UPDATE invoice_sequences SET next_number=next_number+1 WHERE id=?1")
        .bind(sequence.get::<String, _>("id"))
        .execute(&mut *tx)
        .await?;
    let voucher = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO vouchers(id,workspace_id,fiscal_year_id,status,source_type,source_id,posted_at,accounting_date) VALUES(?1,?2,?3,'posted','invoice',?4,CURRENT_TIMESTAMP,?5)")
        .bind(&voucher).bind(workspace).bind(&fy).bind(&draft.id).bind(date).execute(&mut *tx).await?;
    // Freeze the BASELINE fixture arithmetic here rather than routing history
    // through the current recognition calculator/poster. This old rounding is
    // engineering-only and cannot authorize a newly issued invoice.
    let mut buckets = std::collections::BTreeMap::<i64, i64>::new();
    for line in &draft.lines {
        let net = i128::from(line.quantity) * i128::from(line.unit_price_minor);
        let vat = (net * i128::from(line.vat_rate_bp) + 5_000) / 10_000;
        let vat = i64::try_from(vat).expect("historical fixture amount fits i64");
        *buckets.entry(line.vat_rate_bp).or_default() += vat;
    }
    let mut lines = vec![
        ("1510", draft.total_inc_vat_minor, 0, None),
        ("3041", 0, draft.total_ex_vat_minor, None),
    ];
    for (rate, vat) in &buckets {
        let code = match rate {
            0 => "VAT0",
            600 => "VAT6",
            1200 => "VAT12",
            2500 => "VAT25",
            _ => panic!("not an archived historical rate"),
        };
        if *vat > 0 {
            lines.push(("2611", 0, *vat, Some(code)));
        }
    }
    for (account, debit, credit, code) in lines {
        sqlx::query("INSERT INTO journal_lines(id,voucher_id,account_id,debit_minor,credit_minor,vat_code) SELECT ?1,?2,id,?3,?4,?5 FROM accounts WHERE workspace_id=?6 AND number=?7")
            .bind(Uuid::new_v4().to_string()).bind(&voucher).bind(debit).bind(credit).bind(code).bind(workspace).bind(account).execute(&mut *tx).await?;
    }
    // The old engineering version remains addressable even after retirement.
    let rule_id = if year == 2026 {
        "rv-2026-active".to_string()
    } else {
        format!("rv-{year}-test")
    };
    sqlx::query("INSERT INTO invoice_issue_snapshots(invoice_id,workspace_id,business_name,owner_name,tax_status,vat_status,rule_version_id,tax_year,source_url) SELECT ?1,?2,b.business_name,b.owner_name,t.tax_status,v.vat_status,r.id,r.tax_year,r.source_url FROM sole_trader_profiles b JOIN tax_profiles t ON t.workspace_id=b.workspace_id JOIN vat_profiles v ON v.workspace_id=b.workspace_id JOIN rule_versions r ON r.id=?3 WHERE b.workspace_id=?2")
        .bind(&draft.id).bind(workspace).bind(&rule_id).execute(&mut *tx).await?;
    let snapshot = sqlx::query("SELECT business_name,owner_name,tax_status,vat_status,source_url FROM invoice_issue_snapshots WHERE invoice_id=?1")
        .bind(&draft.id).fetch_one(&mut *tx).await?;
    let job = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO local_jobs(id,workspace_id,job_type,status,payload_json) VALUES(?1,?2,'invoice_pdf_generate','queued',?3)")
        .bind(&job).bind(workspace).bind(serde_json::json!({"invoiceId":draft.id,"invoiceNumber":number,"format":"pdf"}).to_string()).execute(&mut *tx).await?;
    sqlx::query("UPDATE invoices SET status='issued',fiscal_year_id=?1,invoice_number=?2,issue_date=?3,voucher_id=?4,pdf_job_id=?5 WHERE id=?6")
        .bind(&fy).bind(&number).bind(date).bind(&voucher).bind(&job).bind(&draft.id).execute(&mut *tx).await?;
    for (action, kind, id, metadata) in [
        (
            "invoice_issue",
            "invoice",
            draft.id.as_str(),
            serde_json::json!({"invoiceNumber":number,"idempotencyKey":input.idempotency_key,"voucherId":voucher,"pdfJobId":job,"businessName":snapshot.get::<String,_>("business_name"),"ownerName":snapshot.get::<String,_>("owner_name"),"taxStatus":snapshot.get::<String,_>("tax_status"),"vatStatus":snapshot.get::<String,_>("vat_status"),"sourceUrl":snapshot.get::<String,_>("source_url"),"ruleVersionId":rule_id,"taxYear":year,"historicalEngineeringFixture":true}),
        ),
        (
            "voucher_post",
            "voucher",
            voucher.as_str(),
            serde_json::json!({"sourceType":"invoice","sourceId":draft.id}),
        ),
    ] {
        sqlx::query("INSERT INTO audit_events(id,workspace_id,action,resource_type,resource_id,metadata_json) VALUES(?1,?2,?3,?4,?5,?6)")
            .bind(Uuid::new_v4().to_string()).bind(workspace).bind(action).bind(kind).bind(id).bind(metadata.to_string()).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    let invoice = invoicing::get_invoice(pool, workspace, &draft.id).await?;
    sqlx::query("INSERT INTO local_jobs(id,workspace_id,job_type,status,payload_json,idempotency_key) VALUES(?1,?2,'invoice_issue','succeeded',?3,?4)")
        .bind(Uuid::new_v4().to_string()).bind(workspace).bind(serde_json::json!({"idempotencyKey":input.idempotency_key,"invoice":invoice}).to_string()).bind(&input.idempotency_key).execute(pool).await?;
    Ok(invoice)
}
