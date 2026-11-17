// Append to baseline src-tauri/tests/invoice_idempotency.rs (44f897e).
#[tokio::test]
async fn new_invoice_requires_case_eligibility_not_just_invoice_method() {
    let (_dir, workspace_id, pool) = setup_workspace().await;
    // Isolate missing facts under the otherwise accepted annual calendar.
    profiles::save_vat_profile(&pool, &workspace_id, &VatProfileSaveInput {
        vat_status: "registered".into(), reporting_period: "yearly".into(),
        accounting_method: "invoice_method".into(), voluntary_registration_date: None,
        vat_filing_deadline_regime: Some("annual_may_12".into()),
    }).await.unwrap();
    let customer = create_customer(&pool, &workspace_id).await;
    let draft = invoicing::create_draft(
        &pool,
        &workspace_id,
        &InvoiceCreateDraftInput {
            counterparty_id: customer.id,
            due_date: None,
            lines: vec![InvoiceLineInput {
                description: "Unreviewed service".to_string(),
                quantity: 1,
                unit_price_minor: 100_000,
                vat_rate: 0.25,
                account_number: Some("3041".to_string()),
            }],
        },
    )
    .await
    .expect("draft retention is not eligibility");
    let error = invoicing::issue_invoice(
        &pool,
        &workspace_id,
        &InvoiceIssueInput {

            invoice_id: draft.id.clone(),
            idempotency_key: "unvalidated-case".to_string(),
            issue_date: Some("2026-02-01".to_string()),
        },
    )
    .await
    .expect_err("stored invoice method alone must not authorize an unreviewed case");
    assert_eq!(error.code, "validation_error");
    let retained = invoicing::get_invoice(&pool, &workspace_id, &draft.id)
        .await
        .unwrap();
    assert_eq!(retained.status, "draft");
    assert!(retained.voucher_id.is_none());
}
