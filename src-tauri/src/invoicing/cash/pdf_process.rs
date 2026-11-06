//! Private exec supervisor. The parser never runs in this process or a DB writer.
//! Capability construction requires the fixed helper's bounded positive hash reply.
use super::{blocked, hash};
use crate::{documents::Document, error::AppError};
use std::{
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tokio::sync::{Semaphore, SemaphorePermit};

static SLOTS: Semaphore = Semaphore::const_new(2);
const WALL: Duration = Duration::from_secs(3);
const MAX_REPLY: u64 = 4096;
const HELPER: &str = if cfg!(windows) {
    "oppenbokforing-pdf-helper.exe"
} else {
    "oppenbokforing-pdf-helper"
};

pub(super) async fn slot() -> Result<SemaphorePermit<'static>, AppError> {
    // Wait with only the small request, before allocating PDF bytes/acquiring the
    // preparation connection. Cancellation of a queued request owns no child.
    SLOTS
        .acquire()
        .await
        .map_err(|_| blocked("PDF helper unavailable"))
}
pub(super) struct PreparedPdf {
    workspace: String,
    document_id: String,
    sha256: String,
}
impl PreparedPdf {
    pub(super) fn matches(&self, workspace: &str, document: &Document) -> Result<(), AppError> {
        if self.workspace != workspace
            || self.document_id != document.id
            || self.sha256 != document.content_sha256
        {
            return Err(blocked("Retained PDF changed after isolated validation"));
        }
        Ok(())
    }
}
fn helper_path() -> Result<PathBuf, AppError> {
    helper_beside(std::env::current_exe().map_err(|_| blocked("PDF helper unavailable"))?)
}
fn helper_beside(exe: PathBuf) -> Result<PathBuf, AppError> {
    let dir = exe
        .parent()
        .ok_or_else(|| blocked("PDF helper unavailable"))?;
    // Cargo integration/unit executables live in debug/deps. This is only a
    // layout adjustment, never a different parser, policy or authorization path.
    #[cfg(debug_assertions)]
    let dir = if dir.file_name().is_some_and(|n| n == "deps") {
        dir.parent()
            .ok_or_else(|| blocked("PDF helper unavailable"))?
    } else {
        dir
    };
    let path = dir.join(HELPER);
    if !path.is_absolute() {
        return Err(blocked("PDF helper path must be absolute"));
    }
    Ok(path)
}
struct OwnedChild {
    child: Child,
    reaped: bool,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.child.kill();
            let _ = self.child.wait(); // Always reap this owned PID, even on error/unwind.
        }
    }
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    version: u32,
    sha256: String,
    valid: bool,
    objects: usize,
    pages: usize,
}
fn verify_reply(bytes: &[u8], digest: &str) -> Result<(), AppError> {
    if bytes.len() as u64 > MAX_REPLY {
        return Err(blocked("PDF helper reply exceeds limit"));
    }
    let reply: Reply =
        serde_json::from_slice(bytes).map_err(|_| blocked("PDF helper reply invalid"))?;
    if reply.version != 1
        || !reply.valid
        || reply.sha256 != digest
        || reply.objects == 0
        || reply.objects > 20_000
        || reply.pages == 0
        || reply.pages > 500
    {
        return Err(blocked("PDF helper did not verify retained bytes"));
    }
    Ok(())
}
fn supervise(path: PathBuf, bytes: &[u8]) -> Result<Vec<u8>, AppError> {
    // Anonymous/unlinked private regular file descriptor: no paths, pipe writer
    // deadlock, auth data or renderer command is passed to the exec child.
    let mut input = tempfile::tempfile().map_err(|_| blocked("PDF helper input unavailable"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        input
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| blocked("PDF helper input unavailable"))?;
    }
    input
        .write_all(bytes)
        .and_then(|_| input.seek(SeekFrom::Start(0)))
        .map_err(|_| blocked("PDF helper input unavailable"))?;
    let deadline = Instant::now() + WALL;
    let child = Command::new(path)
        .env_clear() // Parser needs no inherited secrets, loader overrides or configuration.
        .stdin(Stdio::from(input))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| blocked("Packaged PDF helper missing or unavailable"))?;
    let mut owned = OwnedChild {
        child,
        reaped: false,
    };
    loop {
        if let Some(status) = owned
            .child
            .try_wait()
            .map_err(|_| blocked("PDF helper supervision failed"))?
        {
            owned.reaped = true;
            if !status.success() {
                return Err(blocked("PDF helper rejected structure or resource budget"));
            }
            if Instant::now() >= deadline {
                return Err(blocked("PDF helper exceeded wall deadline"));
            }
            break;
        }
        if Instant::now() >= deadline {
            return Err(blocked("PDF helper exceeded wall deadline"));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // The fixed helper emits at most 4 KiB and has no descendants. A full pipe
    // cannot block supervision: even an overproducing child is killed on deadline.
    let mut output = Vec::new();
    owned
        .child
        .stdout
        .take()
        .ok_or_else(|| blocked("PDF helper reply missing"))?
        .take(MAX_REPLY + 1)
        .read_to_end(&mut output)
        .map_err(|_| blocked("PDF helper reply unavailable"))?;
    Ok(output)
}
pub(super) async fn inspect(
    workspace: &str,
    document: &Document,
    bytes: Vec<u8>,
    permit: SemaphorePermit<'static>,
) -> Result<PreparedPdf, AppError> {
    let workspace = workspace.to_owned();
    let document_id = document.id.clone();
    let sha256 = document.content_sha256.clone();
    checked_process(helper_path()?, bytes, sha256.clone(), permit).await?;
    Ok(PreparedPdf {
        workspace,
        document_id,
        sha256,
    })
}
async fn checked_process(
    path: PathBuf,
    bytes: Vec<u8>,
    sha256: String,
    permit: SemaphorePermit<'static>,
) -> Result<(), AppError> {
    tokio::task::spawn_blocking(move || {
        // The blocking task owns the permit and child cleanup. Dropping/aborting
        // the async caller cannot detach a parser beyond its fixed deadline.
        let _permit = permit;
        if bytes.len() > 10 * 1024 * 1024 || hash(&bytes) != sha256 {
            return Err(blocked("PDF snapshot changed"));
        }
        let output = supervise(path, &bytes)?;
        verify_reply(&output, &sha256)
    })
    .await
    .map_err(|_| blocked("PDF helper supervisor failed"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrong_hash_version_or_malformed_output_cannot_grant_capability() {
        for raw in [
            b"not json".as_slice(),
            br#"{"version":1,"sha256":"wrong","valid":true,"objects":4,"pages":1}"#,
            br#"{"version":2,"sha256":"expected","valid":true,"objects":4,"pages":1}"#,
        ] {
            assert!(verify_reply(raw, "expected").is_err());
        }
        assert!(verify_reply(&vec![b' '; 4097], "expected").is_err());
        assert!(verify_reply(
            br#"{"version":1,"sha256":"expected","valid":true,"objects":4,"pages":1}"#,
            "expected"
        )
        .is_ok());
    }
    #[test]
    fn missing_helper_is_closed() {
        assert!(supervise(
            PathBuf::from("/nonexistent/oppenbokforing-pdf-helper"),
            b"%PDF-"
        )
        .is_err());
    }
    #[cfg(unix)]
    #[test]
    fn crash_timeout_and_drop_are_reaped() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private-test-helper");
        std::fs::write(&path, "#!/bin/sh\nkill -KILL $$\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(supervise(path.clone(), b"%PDF-").is_err());
        std::fs::write(&path, "#!/bin/sh\nprintf malformed\n").unwrap();
        let output = supervise(path.clone(), b"%PDF-").unwrap();
        assert!(verify_reply(&output, "expected").is_err());
        std::fs::write(&path, format!("#!/bin/sh\nprintf '{}'\n", "x".repeat(5000))).unwrap();
        let output = supervise(path.clone(), b"%PDF-").unwrap();
        assert_eq!(output.len() as u64, MAX_REPLY + 1);
        assert!(verify_reply(&output, "expected").is_err());
        let pid_file = dir.path().join("pid");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\necho $$ > '{}'\nexec /bin/sleep 30\n",
                pid_file.display()
            ),
        )
        .unwrap();
        let start = Instant::now();
        assert!(supervise(path, b"%PDF-").is_err());
        assert!(start.elapsed() < WALL + Duration::from_secs(1));
        let timeout_pid: i32 = std::fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_reaped(timeout_pid);
        let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        drop(OwnedChild {
            child,
            reaped: false,
        });
        assert_reaped(pid as i32);
    }
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_caller_keeps_permit_until_child_is_killed_and_reaped() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("private-test-helper");
        let pid_file = dir.path().join("pid");
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\necho $$ > '{}'\nexec /bin/sleep 30\n",
                pid_file.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let bytes = b"%PDF-synthetic-test".to_vec();
        let digest = hash(&bytes);
        let permit = slot().await.unwrap();
        let caller = tokio::spawn(checked_process(path, bytes, digest, permit));
        let start = Instant::now();
        while !pid_file.exists() {
            assert!(start.elapsed() < Duration::from_secs(2));
            std::thread::sleep(Duration::from_millis(10));
        }
        let pid: i32 = std::fs::read_to_string(&pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        caller.abort();
        assert_eq!(
            SLOTS.available_permits(),
            1,
            "cancellation must not free live parser slot"
        );
        while SLOTS.available_permits() != 2 {
            assert!(start.elapsed() < WALL + Duration::from_secs(1));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_reaped(pid);
    }
    #[cfg(unix)]
    fn assert_reaped(pid: i32) {
        let mut status = 0;
        assert_eq!(
            unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
}
