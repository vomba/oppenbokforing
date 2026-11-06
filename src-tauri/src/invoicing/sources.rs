//! Constrained native opening of the invoice validation's compiled official sources.
use serde::Deserialize;
use specta::Type;
use std::process::{Command, Stdio};

use super::validation_registry;
use crate::error::AppError;

const OPEN_FAILED: &str = "Could not open validation source in the system browser";

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InvoiceValidationSourceOpenInput {
    pub validation_id: String,
    pub source_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceManifest {
    validation_id: String,
    source_references: Vec<SourceReference>,
    clock_policy: Option<ClockPolicy>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClockPolicy {
    source_references: Vec<SourceReference>,
}

#[derive(Deserialize)]
struct SourceReference {
    id: String,
    url: String,
}

// Read only the embedded, reviewed manifest, never renderer-supplied URLs or files.
fn resolve_source(input: &InvoiceValidationSourceOpenInput) -> Result<String, AppError> {
    let case = validation_registry::get(&input.validation_id)
        .ok_or_else(|| AppError::validation("Unknown invoice validation", "validationId"))?;
    let mut manifest: SourceManifest =
        serde_json::from_str(case.manifest).map_err(|_| AppError::internal(OPEN_FAILED))?;
    if manifest.validation_id != case.id {
        return Err(AppError::internal(OPEN_FAILED));
    }
    if let Some(clock) = manifest.clock_policy {
        manifest.source_references.extend(clock.source_references);
    }
    let source = manifest
        .source_references
        .into_iter()
        .find(|source| source.id == input.source_id)
        .ok_or_else(|| AppError::validation("Unknown validation source", "sourceId"))?;
    // Source allowlist contract: invoice-lifecycle.
    if !(source.url.starts_with("https://www.bfn.se/")
        || source.url.starts_with("https://www.skatteverket.se/")
        || source.url.starts_with("https://www.riksdagen.se/")
        || source.url.starts_with("https://eur-lex.europa.eu/"))
        || source.url.bytes().any(|byte| {
            !byte.is_ascii()
                || byte.is_ascii_whitespace()
                || byte.is_ascii_control()
                || byte == b'\\'
        })
    {
        return Err(AppError::internal(OPEN_FAILED));
    }
    Ok(source.url)
}

#[derive(Clone, Copy)]
enum OpenPlatform {
    MacOs,
    Unsupported,
}

impl OpenPlatform {
    fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Unsupported
        }
    }
}

async fn open_source_with<F>(
    input: InvoiceValidationSourceOpenInput,
    platform: OpenPlatform,
    run: F,
) -> Result<(), AppError>
where
    F: FnOnce(&mut Command) -> std::io::Result<bool> + Send + 'static,
{
    let url = resolve_source(&input)?;
    tokio::task::spawn_blocking(move || match platform {
        OpenPlatform::MacOs => {
            let mut command = Command::new("/usr/bin/open");
            command
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if !run(&mut command).map_err(|_| AppError::internal(OPEN_FAILED))? {
                return Err(AppError::internal(OPEN_FAILED));
            }
            Ok(())
        }
        OpenPlatform::Unsupported => Err(AppError::internal(
            "Opening validation sources is not supported on this platform",
        )),
    })
    .await
    .map_err(|_| AppError::internal(OPEN_FAILED))?
}

/// Open an exact compiled source in the supported system browser.
pub async fn invoice_validation_source_open(
    input: &InvoiceValidationSourceOpenInput,
) -> Result<(), AppError> {
    open_source_with(input.clone(), OpenPlatform::current(), |command| {
        command.status().map(|status| status.success())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const VALIDATION: &str = "invoice-recognition-se-service25-2026-exact-v1";
    const SOURCES: [(&str, &str); 4] = [
        ("bfn-timing", "https://www.bfn.se/wp-content/uploads/vl13-2-bokforing.pdf"),
        ("skv-2026-method", "https://www.skatteverket.se/download/18.9567cda19bf6c0027216f6/1770379535285/bokforing-bokslut-och-deklaration-skv282utgava09.pdf"),
        ("skv-2026-rate", "https://www.skatteverket.se/download/18.1522bf3f19aea8075ba3285/1767885159120/belopp-och-procentsatser-for-inkomstaaret-2026.pdf"),
        ("skv-annual-no-eu", "https://www.skatteverket.se/foretag/moms/deklareramoms/narskajagdeklareramoms.4.6d02084411db6e252fe80008988.html"),
    ];

    fn input(validation: &str, source: &str) -> InvoiceValidationSourceOpenInput {
        InvoiceValidationSourceOpenInput {
            validation_id: validation.into(),
            source_id: source.into(),
        }
    }

    #[test]
    fn input_requires_camel_case_ids_and_rejects_unknown_fields() {
        let value = serde_json::json!({"validationId": VALIDATION, "sourceId": "bfn-timing"});
        let parsed: InvoiceValidationSourceOpenInput =
            serde_json::from_value(value.clone()).unwrap();
        assert_eq!(parsed.validation_id, VALIDATION);
        assert_eq!(parsed.source_id, "bfn-timing");
        for extra in ["url", "path", "command"] {
            let mut supplied = value.clone();
            supplied[extra] = "https://evil.invalid/".into();
            assert!(serde_json::from_value::<InvoiceValidationSourceOpenInput>(supplied).is_err());
        }
        for invalid in [
            serde_json::json!({"validation_id": VALIDATION, "source_id": "bfn-timing"}),
            serde_json::json!({"validationId": VALIDATION}),
            serde_json::json!({"sourceId": "bfn-timing"}),
        ] {
            assert!(serde_json::from_value::<InvoiceValidationSourceOpenInput>(invalid).is_err());
        }
    }

    #[tokio::test]
    async fn allowlisted_sources_dispatch_exact_macos_command_off_async_thread() {
        let async_thread = std::thread::current().id();
        for (id, expected_url) in SOURCES {
            let calls = Arc::new(Mutex::new(Vec::new()));
            let captured = calls.clone();
            open_source_with(input(VALIDATION, id), OpenPlatform::MacOs, move |command| {
                assert_ne!(std::thread::current().id(), async_thread);
                captured.lock().unwrap().push((
                    command.get_program().to_string_lossy().into_owned(),
                    command
                        .get_args()
                        .map(|arg| arg.to_string_lossy().into_owned())
                        .collect::<Vec<_>>(),
                ));
                Ok(true)
            })
            .await
            .unwrap();
            assert_eq!(
                *calls.lock().unwrap(),
                vec![("/usr/bin/open".to_string(), vec![expected_url.to_string()])]
            );
        }
    }

    #[tokio::test]
    async fn successor_invoice_and_cash_tax_and_clock_ids_open_exact_compiled_urls() {
        for (id, url) in SOURCES {
            assert_eq!(
                resolve_source(&input(validation_registry::INVOICE_V2.id, id)).unwrap(),
                url
            );
        }
        let case = &validation_registry::CASH_V1;
        let manifest: serde_json::Value = serde_json::from_str(case.manifest).unwrap();
        for source in manifest["sourceReferences"]
            .as_array()
            .unwrap()
            .iter()
            .chain(
                manifest["clockPolicy"]["sourceReferences"]
                    .as_array()
                    .unwrap(),
            )
        {
            let expected_url = source["url"].as_str().unwrap().to_string();
            open_source_with(
                input(case.id, source["id"].as_str().unwrap()),
                OpenPlatform::MacOs,
                move |command| {
                    assert_eq!(command.get_program(), "/usr/bin/open");
                    assert_eq!(
                        command.get_args().collect::<Vec<_>>(),
                        vec![std::ffi::OsStr::new(&expected_url)]
                    );
                    Ok(true)
                },
            )
            .await
            .unwrap();
        }
        for case in [
            &validation_registry::INVOICE_V1,
            &validation_registry::INVOICE_V2,
        ] {
            let error = open_source_with(
                input(case.id, "swedish-standard-time"),
                OpenPlatform::MacOs,
                |_| panic!("clock IDs cannot cross case boundaries"),
            )
            .await
            .unwrap_err();
            assert_eq!(error.details.unwrap()[0].field.as_deref(), Some("sourceId"));
        }
    }

    #[tokio::test]
    async fn unknown_ids_and_payloads_never_reach_launcher() {
        let payloads = [
            "",
            "unknown",
            "BFN-TIMING",
            " bfn-timing",
            "bfn-timing ",
            "https://www.bfn.se/wp-content/uploads/vl13-2-bokforing.pdf",
            "https://evil.invalid/",
            "file:///etc/passwd",
            "/tmp/source.pdf",
            "javascript:alert(1)",
            "$(touch /tmp/source-open)",
            "; open /tmp/source.pdf",
            "--args",
            "bfn-timing\nopen /tmp/source.pdf",
            "bfn-timing\0",
        ];
        for payload in payloads {
            for (request, field) in [
                (input(payload, "bfn-timing"), "validationId"),
                (input(VALIDATION, payload), "sourceId"),
            ] {
                let error = open_source_with(request, OpenPlatform::MacOs, |_| {
                    panic!("invalid IDs must never reach OS I/O")
                })
                .await
                .unwrap_err();
                assert_eq!(error.code, "validation_error");
                assert_eq!(error.details.unwrap()[0].field.as_deref(), Some(field));
            }
        }
        let error = open_source_with(
            input(&VALIDATION.to_uppercase(), "bfn-timing"),
            OpenPlatform::MacOs,
            |_| panic!("case must match exactly"),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "validation_error");
    }

    #[tokio::test]
    async fn launch_io_and_exit_failures_are_redacted() {
        for io_failure in [false, true] {
            let error = open_source_with(
                input(VALIDATION, "bfn-timing"),
                OpenPlatform::MacOs,
                move |_| {
                    if io_failure {
                        Err(std::io::Error::other(
                            "secret /Users/private/path; process stderr",
                        ))
                    } else {
                        Ok(false)
                    }
                },
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, "internal_error");
            assert_eq!(
                error.message,
                "Could not open validation source in the system browser"
            );
            assert!(error.details.is_none());
        }
    }

    #[tokio::test]
    async fn worker_failure_is_redacted() {
        let error = open_source_with(input(VALIDATION, "bfn-timing"), OpenPlatform::MacOs, |_| {
            panic!("private process details")
        })
        .await
        .unwrap_err();
        assert_eq!(
            error.message,
            "Could not open validation source in the system browser"
        );
        assert!(error.details.is_none());
    }

    #[tokio::test]
    async fn unsupported_platform_reports_error_without_launching() {
        let error = open_source_with(
            input(VALIDATION, "bfn-timing"),
            OpenPlatform::Unsupported,
            |_| panic!("unsupported platform must not invoke a launcher"),
        )
        .await
        .unwrap_err();
        assert!(error.message.contains("not supported on this platform"));
        assert_eq!(error.code, "internal_error");
    }
}
