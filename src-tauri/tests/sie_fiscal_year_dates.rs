//! Public SIE regressions: real reviewed first-year dates, not renderer-derived expectations.
#[path = "cash_receipt_recognition/fixture.rs"]
mod fixture;

use fixture::CashFixture;
use oppenbokforing_desktop_lib::{
    db::connect_workspace,
    documents,
    invoicing::cash::{self, BankReceiptIdentity, CashReceiptFacts, CashReceiptRecordInput},
    sie::{sie_export_create, SieExportCreateInput},
};
use sqlx::SqlitePool;
use std::{fs, path::PathBuf};

fn input(key: &str) -> SieExportCreateInput {
    SieExportCreateInput {
        fiscal_year: 2026,
        idempotency_key: key.into(),
        export_directory: None,
    }
}

// Observe all durable rows, including rules, fiscal metadata, issued identity,
// evidence and ledger. Successful exports exclude only expected SIE audit/job
// writes; failed exports exclude nothing. Typed hex preserves BLOB comparisons.
async fn durable_state(pool: &SqlitePool, exclude_sie_writes: bool) -> Vec<(String, Vec<String>)> {
    let tables: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .fetch_all(pool)
            .await
            .unwrap();
    let mut state = Vec::new();
    for table in tables {
        let columns: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
        ))
        .fetch_all(pool)
        .await
        .unwrap();
        let columns = columns
            .iter()
            .map(|c| format!("json_array(typeof(\"{c}\"), CASE WHEN typeof(\"{c}\")='blob' THEN hex(\"{c}\") ELSE \"{c}\" END)"))
            .collect::<Vec<_>>()
            .join(",");
        let filter = match table.as_str() {
            "local_jobs" if exclude_sie_writes => " WHERE job_type!='sie_export_create'",
            "audit_events" if exclude_sie_writes => " WHERE action!='sie_export_create'",
            _ => "",
        };
        let rows = sqlx::query_scalar(&format!(
            "SELECT json_array({columns}) FROM \"{table}\"{filter} ORDER BY 1"
        ))
        .fetch_all(pool)
        .await
        .unwrap();
        state.push((table, rows));
    }
    state
}

async fn export(fixture: &CashFixture, key: &str) -> PathBuf {
    let before = durable_state(&fixture.pool, true).await;
    let summary = sie_export_create(&fixture.pool, &fixture.workspace, &input(key))
        .await
        .expect("public SIE export");
    assert_eq!(durable_state(&fixture.pool, true).await, before);
    let path = PathBuf::from(&summary.export_path);
    if path.is_absolute() {
        path
    } else {
        fixture.dir.path().join("exports").join(path)
    }
}

fn header(path: &std::path::Path) -> String {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .find(|line| line.starts_with("#RAR "))
        .expect("actual file has fiscal-year header")
        .to_owned()
}

async fn receive(fixture: &CashFixture, short_year: bool) {
    let date = if short_year {
        "2026-06-20"
    } else {
        "2026-01-20"
    };
    let pdf = if short_year {
        fixture::synthetic_bank_pdf_for(date, "SYNTHETIC-SIE-JUNE-RECEIPT", "2026-0001")
    } else {
        fixture::synthetic_bank_pdf()
    };
    let document = documents::store_document_bytes(
        &fixture.pool,
        &fixture.workspace,
        &pdf,
        "SYNTHETIC-sie-receipt.pdf",
        "application/pdf",
    )
    .await
    .unwrap();
    cash::invoice_cash_receipt_record(
        &fixture.pool,
        &fixture.workspace,
        &CashReceiptRecordInput {
            invoice_id: fixture.issued.id.clone(),
            document_id: document.id,
            idempotency_key: "sie-public-receipt".into(),
            facts: CashReceiptFacts {
                actual_received_amount_minor: Some(1_250_000),
                actual_received_on: Some(date.into()),
                currency: Some("SEK".into()),
                received_already: Some(true),
                bank_receipt_identity: Some(BankReceiptIdentity {
                    bank_id: "SYNTHETIC-TEST-BANK".into(),
                    receiving_account_reference: "SYNTHETIC-BUSINESS-ACCOUNT-001".into(),
                    transaction_reference: if short_year {
                        "SYNTHETIC-SIE-JUNE-RECEIPT"
                    } else {
                        "SYNTHETIC-TXN-20260120-001"
                    }
                    .into(),
                }),
                source_entry_locator: Some("page 1".into()),
                bank_pdf_matches_receipt_confirmed: Some(true),
                issued_case_still_applies_confirmed: Some(true),
                no_other_receipts_or_adjustments_confirmed: Some(true),
            },
        },
    )
    .await
    .expect("public full receipt with actual synthetic source facts");
}

#[tokio::test]
async fn approved_short_first_year_exports_actual_bounds_and_regenerates_after_reopen() {
    let fixture = CashFixture::issued_dates("2026-06-01", "2026-06-10", "2026-06-15").await;
    receive(&fixture, true).await;
    let stored: (String, String) = sqlx::query_as(
        "SELECT starts_on,ends_on FROM fiscal_years WHERE workspace_id=?1 AND id='fy-' || ?1 || '-2026'",
    )
    .bind(&fixture.workspace)
    .fetch_one(&fixture.pool)
    .await
    .unwrap();
    assert_eq!(stored, ("2026-06-01".into(), "2026-12-31".into()));
    let path = export(&fixture, "short-first-year").await;
    println!(
        "PUBLIC SIE: reviewed/stored June 1 to December 31; actual file header: {}",
        header(&path)
    );
    assert_eq!(header(&path), "#RAR 0 20260601 20261231");
    let bytes = fs::read(&path).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("#VER \"A\" \"1\" 20260620"));
    assert!(String::from_utf8_lossy(&bytes).contains("#TRANS 1930 {} 12500.00 20260620"));
    assert!(String::from_utf8_lossy(&bytes).contains("#TRANS 2611 {} -2500.00 20260620"));
    assert!(String::from_utf8_lossy(&bytes).contains("#TRANS 3041 {} -10000.00 20260620"));
    assert_eq!(export(&fixture, "short-first-year").await, path);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fixture.pool.close().await;
    let reopened = CashFixture {
        pool: connect_workspace(&fixture.dir.path().join("workspace.sqlite"))
            .await
            .unwrap(),
        ..fixture
    };
    fs::remove_file(&path).unwrap();
    export(&reopened, "short-first-year").await;
    assert_eq!(header(&path), "#RAR 0 20260601 20261231");
}

#[tokio::test]
async fn existing_calendar_artifact_is_preserved_but_missing_file_uses_selected_metadata() {
    let fixture = CashFixture::issued().await;
    receive(&fixture, false).await;
    let path = export(&fixture, "calendar-preserved").await;
    assert_eq!(header(&path), "#RAR 0 20260101 20261231");
    let bytes = fs::read(&path).unwrap();
    let published_dir = fixture.dir.path().join("published");
    fs::create_dir_all(&published_dir).unwrap();
    let mut publish_input = input("calendar-preserved");
    publish_input.export_directory = Some(published_dir.to_string_lossy().into());
    let published = sie_export_create(&fixture.pool, &fixture.workspace, &publish_input)
        .await
        .unwrap();
    assert_eq!(fs::read(&published.export_path).unwrap(), bytes);
    // Deliberate damaged/changed metadata simulation, NOT cash-origin setup or authority.
    sqlx::query("UPDATE fiscal_years SET starts_on='2026-06-01' WHERE workspace_id=?1 AND id='fy-' || ?1 || '-2026'")
        .bind(&fixture.workspace).execute(&fixture.pool).await.unwrap();
    export(&fixture, "calendar-preserved").await;
    assert_eq!(
        fs::read(&path).unwrap(),
        bytes,
        "existing January artifact must remain byte-identical"
    );
    let before = durable_state(&fixture.pool, true).await;
    sie_export_create(&fixture.pool, &fixture.workspace, &publish_input)
        .await
        .unwrap();
    assert_eq!(durable_state(&fixture.pool, true).await, before);
    assert_eq!(
        fs::read(&published.export_path).unwrap(),
        bytes,
        "published retry must copy the existing bytes, not render today's narrower dates"
    );
    fs::remove_file(&path).unwrap();
    export(&fixture, "calendar-preserved").await;
    assert_eq!(header(&path), "#RAR 0 20260601 20261231");
    assert_eq!(
        fs::read(&published.export_path).unwrap(),
        bytes,
        "regenerating the missing workspace file does not replace an older external copy"
    );
}

#[tokio::test]
async fn malformed_reversed_or_foreign_year_metadata_fails_closed() {
    let fixture = CashFixture::issued_dates("2026-06-01", "2026-06-10", "2026-06-15").await;
    let missing_path = export(&fixture, "missing-invalid-year").await;
    fs::remove_file(&missing_path).unwrap();
    for (start, end) in [
        ("", "2026-12-31"),
        ("2026-6-01", "2026-12-31"),
        ("2026-02-30", "2026-12-31"),
        ("2026-06-01", "2026-13-01"),
        ("2026-06-01", ""),
        ("2026-12-31", "2026-06-01"),
    ] {
        sqlx::query("UPDATE fiscal_years SET starts_on=?1,ends_on=?2 WHERE workspace_id=?3 AND id='fy-' || ?3 || '-2026'")
            .bind(start).bind(end).bind(&fixture.workspace).execute(&fixture.pool).await.unwrap();
        let before = durable_state(&fixture.pool, false).await;
        for key in ["invalid-year", "missing-invalid-year"] {
            let error = sie_export_create(&fixture.pool, &fixture.workspace, &input(key))
                .await
                .expect_err(
                    "new export and missing-file regeneration must reject invalid metadata",
                );
            assert_eq!(error.code, "validation_error");
            assert_eq!(durable_state(&fixture.pool, false).await, before);
            assert!(!missing_path.exists());
        }
    }
    // Existing helper resolves a deterministic ID: an ID alone is not scope authority.
    sqlx::query("INSERT INTO workspaces(id,name,database_path,documents_path,exports_path) VALUES('foreign','foreign','unused','unused','unused')")
        .execute(&fixture.pool).await.unwrap();
    sqlx::query("INSERT INTO workspaces(id,name,database_path,documents_path,exports_path) VALUES('requester','requester','requester-unused','requester-unused','requester-unused')")
        .execute(&fixture.pool).await.unwrap();
    sqlx::query("INSERT INTO fiscal_years(id,workspace_id,starts_on,ends_on,status) VALUES('fy-requester-2026','foreign','2026-06-01','2026-12-31','open')")
        .execute(&fixture.pool).await.unwrap();
    let before = durable_state(&fixture.pool, false).await;
    let error = sie_export_create(&fixture.pool, "requester", &input("foreign-year"))
        .await
        .expect_err("foreign fiscal-year row must fail closed");
    assert_eq!(error.code, "validation_error");
    assert_eq!(durable_state(&fixture.pool, false).await, before);
    assert!(!missing_path.exists());
}
