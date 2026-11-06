//! Exec-only parser boundary. No application library, Tauri, DB, paths or network.
//! Current printpdf/lopdf uses Rust flate2/miniz, no rayon/C zlib. Re-audit budget
//! coverage if that dependency feature graph changes. No args/env tune the limits.
//! Fixed envelope: 128 MiB requested Rust heap from startup, verified self-VM +
//! 256 MiB AS and 2 CPU seconds before input. Parent enforces 3 wall seconds and
//! two concurrent helpers. AS/heap are not a universal RSS or stack-byte ceiling.
//! Unsupported OS/failed OS-limit setup rejects input, never parses unbounded.
mod budget;
mod limits;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

#[global_allocator]
static HEAP: budget::Budget = budget::Budget::new(128 * 1024 * 1024);
const MAX_INPUT: u64 = 10 * 1024 * 1024;

fn inspect() -> Result<serde_json::Value, ()> {
    limits::install()?;
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_INPUT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAX_INPUT || !bytes.starts_with(b"%PDF-") {
        return Err(());
    }
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let pdf = printpdf::lopdf::Document::load_mem(&bytes).map_err(|_| ())?;
    let objects = pdf.objects.len();
    if pdf.is_encrypted() || objects > 20_000 {
        return Err(());
    }
    let pages = pdf.get_pages();
    if pages.is_empty() || pages.len() > 500 {
        return Err(());
    }
    let content = pages.values().any(|id| {
        pdf.get_page_contents(*id).iter().any(|content_id| {
            pdf.get_object(*content_id)
                .ok()
                .and_then(|o| o.as_stream().ok())
                .is_some_and(|s| !s.content.is_empty())
        })
    });
    if !content {
        return Err(());
    }
    Ok(
        serde_json::json!({"version":1,"sha256":digest,"valid":true,"objects":objects,"pages":pages.len()}),
    )
}
fn main() {
    // No panic/parse diagnostics (or untrusted data) on stderr. Parent also nulls it.
    std::panic::set_hook(Box::new(|_| {}));
    let Ok(reply) = inspect() else {
        std::process::exit(2);
    };
    let Ok(encoded) = serde_json::to_vec(&reply) else {
        std::process::exit(2);
    };
    if encoded.len() > 4096 || std::io::stdout().lock().write_all(&encoded).is_err() {
        std::process::exit(2);
    }
}
