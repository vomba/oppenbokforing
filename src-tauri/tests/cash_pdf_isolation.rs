//! Synthetic adversarial files. The public service is probed in an owned exec child
//! so a regression to the old in-process parser cannot hang/abort the test runner.
//! Hand-authored synthetic fixture structure (no spike sources are used): ordinary
//! page/content objects, classic xref; /Prev points to that same xref. ObjStm's
//! zlib-decoded bytes are `6 0 42 ` followed by either zero or 512 MiB of spaces.
//! Both compressed files have identical PDF structure; only expansion differs.
#[path = "cash_receipt_recognition/fixture.rs"]
mod fixture;
use fixture::{assert_no_recognition, CashFixture};
use oppenbokforing_desktop_lib::{
    documents,
    invoicing::cash::{self, CashReceiptRecordInput},
};
use serde_json::json;
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn run_probe(mode: &str) {
    let isolated = tempfile::tempdir().unwrap();
    let mut executable = std::env::current_exe().unwrap();
    if mode == "missing" {
        // Exercise actual fixed-path discovery without renaming a shared build
        // artifact or introducing any production environment/test override.
        let relocated = isolated.path().join(executable.file_name().unwrap());
        std::fs::copy(&executable, &relocated).unwrap();
        executable = relocated;
    }
    run_executable_probe(&executable, mode);
}
fn run_executable_probe(executable: &std::path::Path, mode: &str) {
    let mut child = Command::new(executable)
        .args([
            "--ignored",
            "--exact",
            "public_service_probe",
            "--nocapture",
        ])
        .env("PDF_ISOLATION_TEST_CASE", mode)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            println!(
                "probe={mode} pid={} status={status} elapsed={:?} reaped=true",
                child.id(),
                start.elapsed()
            );
            assert!(
                status.success(),
                "public service isolation probe {mode} failed"
            );
            break;
        }
        if start.elapsed() > Duration::from_secs(10) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!(
                "probe {mode}: outer deadline; killed and reaped PID {}",
                child.id()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn cyclic_pdf_cannot_lock_accounting_writer() {
    run_probe("cycle");
}
#[test]
fn compressed_bomb_cannot_abort_accounting_process() {
    run_probe("bomb");
}
#[test]
fn normal_compressed_object_stream_remains_supported() {
    run_probe("compressed");
}
#[test]
fn normal_wellformed_pdf_remains_supported() {
    run_probe("normal");
}
#[test]
fn malformed_pdf_denies_receipt_without_writes() {
    run_probe("malformed");
}
#[test]
fn missing_fixed_path_helper_denies_new_receipt_without_writes() {
    run_probe("missing");
}

#[test]
fn cargo_builds_real_helper() {
    let built = std::path::Path::new(env!("CARGO_BIN_EXE_oppenbokforing-pdf-helper"));
    assert!(std::fs::canonicalize(built).unwrap().is_file());
    let input = tempfile::tempfile().unwrap();
    use std::io::{Seek, Write};
    let mut input = input;
    input.write_all(&fixture::synthetic_bank_pdf()).unwrap();
    input.rewind().unwrap();
    let output = Command::new(built).stdin(input).output().unwrap();
    assert!(output.status.success());
    let reply: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(reply["version"], 1);
    assert_eq!(reply["valid"], true);
    assert_eq!(reply["pages"], 1);
    assert!(output.stdout.len() <= 4096);
}

#[cfg(debug_assertions)]
#[test]
fn modern_cargo_layout_uses_real_profile_helper_without_adjacent_fallback() {
    let isolated = tempfile::tempdir().unwrap();
    let profile = isolated.path().join("debug");
    let out = profile.join("build/oppenbokforing-desktop/0123456789abcdef/out");
    std::fs::create_dir_all(&out).unwrap();
    let executable = out.join(std::env::current_exe().unwrap().file_name().unwrap());
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let built = std::path::Path::new(env!("CARGO_BIN_EXE_oppenbokforing-pdf-helper"));
    let name = if cfg!(windows) {
        "oppenbokforing-pdf-helper.exe"
    } else {
        "oppenbokforing-pdf-helper"
    };
    let helper = profile.join(name);
    std::fs::copy(built, &helper).unwrap();
    run_executable_probe(&executable, "normal");

    // A missing fixed profile helper must not fall back to another location,
    // even when the real parser is present beside the Cargo test executable.
    std::fs::copy(&helper, out.join(name)).unwrap();
    std::fs::remove_file(helper).unwrap();
    run_executable_probe(&executable, "missing");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "owned by the bounded parent harness"]
async fn public_service_probe() {
    let mode = std::env::var("PDF_ISOLATION_TEST_CASE").unwrap();
    let f = CashFixture::issued().await;
    let normal = fixture::synthetic_bank_pdf();
    let bytes: &[u8] = match mode.as_str() {
        "normal" | "missing" => &normal,
        "malformed" => b"%PDF-1.4\n%%EOF\n",
        "cycle" => include_bytes!("cash_pdf_isolation/prev-cycle.pdf"),
        "bomb" => include_bytes!("cash_pdf_isolation/compressed-512mib.pdf"),
        "compressed" => include_bytes!("cash_pdf_isolation/compressed-small.pdf"),
        _ => panic!("unknown test case"),
    };
    let doc = documents::store_document_bytes(
        &f.pool,
        &f.workspace,
        bytes,
        "synthetic-isolation.pdf",
        "application/pdf",
    )
    .await
    .unwrap();
    let input: CashReceiptRecordInput = serde_json::from_value(json!({"invoiceId":f.issued.id,"documentId":doc.id,"idempotencyKey":"isolation-probe","facts":{
        "actualReceivedAmountMinor":1250000,"actualReceivedOn":"2026-01-20","currency":"SEK","receivedAlready":true,
        "bankReceiptIdentity":{"bankId":"SYNTHETIC","receivingAccountReference":"SYNTHETIC-ACCOUNT","transactionReference":"SYNTHETIC-TXN"},
        "sourceEntryLocator":"synthetic page 1","bankPdfMatchesReceiptConfirmed":true,"issuedCaseStillAppliesConfirmed":true,"noOtherReceiptsOrAdjustmentsConfirmed":true}})).unwrap();
    // Truly independent pool: the service pool intentionally has only one connection.
    let observer = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(f.dir.path().join("workspace.sqlite"))
                .busy_timeout(Duration::from_millis(150)),
        )
        .await
        .unwrap();
    let tables = [
        "cash_receipt_recognitions",
        "reconciliation_matches",
        "staged_transactions",
        "vouchers",
        "journal_lines",
        "local_jobs",
        "audit_events",
        "documents",
    ];
    let mut before = Vec::new();
    for table in tables {
        before.push(
            sqlx::query_scalar::<_, i64>(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&f.pool)
                .await
                .unwrap(),
        );
    }
    let pool = f.pool.clone();
    let workspace = f.workspace.clone();
    let start = Instant::now();
    let operation =
        tokio::spawn(
            async move { cash::invoice_cash_receipt_record(&pool, &workspace, &input).await },
        );
    if mode == "cycle" {
        // Separate physical SQLite connection, with its own short busy deadline.
        std::thread::sleep(Duration::from_millis(250));
        let mut conn = observer.acquire().await.unwrap();
        sqlx::query("PRAGMA busy_timeout=150")
            .execute(&mut *conn)
            .await
            .unwrap();
        let writer = sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await;
        if let Err(error) = writer {
            eprintln!("SECURITY REGRESSION: retained PDF parser holds accounting writer: {error}");
            std::process::exit(31); // Do not hang dropping the old stuck parser's runtime.
        }
        sqlx::query("ROLLBACK").execute(&mut *conn).await.unwrap();
        assert!(
            !operation.is_finished(),
            "fixture must exercise a still-running malicious parse"
        );
        println!("independent BEGIN IMMEDIATE available DURING malicious parse");
        let read_start = Instant::now();
        oppenbokforing_desktop_lib::profiles::get_vat_profile(&f.pool, &f.workspace)
            .await
            .unwrap();
        assert!(
            read_start.elapsed() < Duration::from_millis(250),
            "normal method read waited on PDF deadline"
        );
    }
    let result = operation.await.unwrap();
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "service deadline not enforced"
    );
    if mode == "compressed" || mode == "normal" {
        result.expect("ordinary PDFs including compressed ObjStm remain supported");
    } else {
        if mode == "missing" {
            assert_eq!(
                result.as_ref().unwrap_err().message,
                "Packaged PDF helper missing or unavailable"
            );
        }
        assert!(result.is_err(), "malicious PDF granted receipt proof");
        assert_no_recognition(&f.pool, &f.workspace).await;
        for table in [
            "cash_receipt_recognitions",
            "reconciliation_matches",
            "staged_transactions",
        ] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&f.pool)
                .await
                .unwrap();
            assert_eq!(count, 0, "failure leaked {table}");
        }
        for (index, table) in tables.into_iter().enumerate() {
            let after: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&f.pool)
                .await
                .unwrap();
            assert_eq!(after, before[index], "failed parse changed {table}");
        }
        documents::verify_retained_document(&f.pool, &f.workspace, &doc.id)
            .await
            .expect("evidence kept");
    }
}
