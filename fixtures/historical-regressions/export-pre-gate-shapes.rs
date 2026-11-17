// Append to baseline src-tauri/tests/invoice_idempotency.rs (44f897e).
#[tokio::test]
async fn export_pre_gate_historical_shapes() {
    let mut cases = Vec::new();
    for (name,rates) in [("exempt",vec![0.0]),("six",vec![0.06]),("twelve",vec![0.12]),("standard",vec![0.25]),("mixed",vec![0.25,0.12])] {
        let (_dir, workspace, pool) = setup_workspace().await;
        if name == "exempt" {
            profiles::save_vat_profile(&pool,&workspace,&VatProfileSaveInput {
                vat_status:"exempt_low_turnover".into(),reporting_period:"quarterly".into(),accounting_method:"invoice_method".into(),voluntary_registration_date:None,vat_filing_deadline_regime:None,
            }).await.unwrap();
        }
        let customer=create_customer(&pool,&workspace).await;
        let draft=invoicing::create_draft(&pool,&workspace,&InvoiceCreateDraftInput { counterparty_id:customer.id,due_date:None,
            lines:rates.iter().map(|rate| InvoiceLineInput {description:"Historical service".into(),quantity:1,unit_price_minor:100000,vat_rate:*rate,account_number:Some("3041".into())}).collect()
        }).await.unwrap();
        let issued=invoicing::issue_invoice(&pool,&workspace,&InvoiceIssueInput {invoice_id:draft.id,idempotency_key:format!("archive-{name}"),issue_date:Some("2026-02-01".into())}).await.unwrap();
        let journal:Vec<(String,i64,i64,Option<String>)>=sqlx::query_as("SELECT a.number,j.debit_minor,j.credit_minor,j.vat_code FROM journal_lines j JOIN accounts a ON a.id=j.account_id WHERE j.voucher_id=?1 ORDER BY a.number,j.vat_code").bind(issued.voucher_id.as_deref().unwrap()).fetch_all(&pool).await.unwrap();
        let jobs:Vec<(String,String)>=sqlx::query_as("SELECT job_type,status FROM local_jobs WHERE workspace_id=?1 ORDER BY job_type").bind(&workspace).fetch_all(&pool).await.unwrap();
        let snapshot:(String,i64,String)=sqlx::query_as("SELECT rule_version_id,tax_year,vat_status FROM invoice_issue_snapshots WHERE invoice_id=?1").bind(&issued.id).fetch_one(&pool).await.unwrap();
        let audit:Vec<String>=sqlx::query_scalar("SELECT action FROM audit_events WHERE resource_id IN (?1,?2) AND action IN ('invoice_issue','voucher_post') ORDER BY action").bind(&issued.id).bind(issued.voucher_id.as_deref()).fetch_all(&pool).await.unwrap();
        cases.push(serde_json::json!({"id":name,"vatRates":rates,"unitPriceMinorPerLine":100000,"netMinor":issued.total_ex_vat_minor,"vatMinor":issued.total_vat_minor,"grossMinor":issued.total_inc_vat_minor,"invoiceNumber":issued.invoice_number,"issueDate":issued.issue_date,"journal":journal,"jobs":jobs,"snapshot":snapshot,"audit":audit}));
    }
    println!("HISTORICAL_SHAPES={}",serde_json::json!({"sourceCommit":"44f897e","classification":"engineering_regression_not_independent_validation","cases":cases}));
}
