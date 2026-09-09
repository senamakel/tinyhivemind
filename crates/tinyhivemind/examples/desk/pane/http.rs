//! The three HTTP calls a pane needs, over `curl`.
//!
//! Same reason [`crate::chat`] and [`crate::memory`] shell out: this example
//! is the host written out in full, and adding an HTTP client to a workspace
//! whose whole point is that the library takes no transport would be a poor
//! advertisement for it. `curl` is already a hard dependency of both.

use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

/// `GET` a path on a pane's server, returning the body or `None`.
pub(crate) fn get(base: &str, path: &str, timeout: Duration) -> Option<String> {
    let output = Command::new("curl")
        .args(["-s", "-m", &timeout.as_secs().to_string()])
        .arg(format!("{base}{path}"))
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string()
    })
}

/// `POST` a JSON body to a path on a pane's server.
///
/// The body is written on stdin rather than passed as an argument: a turn
/// prompt is twenty kilobytes of text, and `ARG_MAX` is not a limit worth
/// discovering the first time a desk grows a long standing account.
pub(crate) fn post(base: &str, path: &str, body: &str, timeout: Duration) -> Option<String> {
    let mut child = Command::new("curl")
        .args([
            "-s",
            "-m",
            &timeout.as_secs().to_string(),
            "-X",
            "POST",
            "-H",
            "content-type: application/json",
            "--data-binary",
            "@-",
        ])
        .arg(format!("{base}{path}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(body.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string()
    })
}

/// Start a detached `curl` that appends a server's event stream to a file.
///
/// The stream is the pane's own record of what its seat did, and reading it
/// from a byte offset is what lets one turn see only its own events without
/// the host holding a socket open across the run.
///
/// # Errors
///
/// Returns the spawn failure.
pub(crate) fn follow_events(
    base: &str,
    into: &Path,
) -> Result<std::process::Child, std::io::Error> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(into)?;
    Command::new("curl")
        .args(["-s", "-N", "--no-buffer"])
        .arg(format!("{base}/event"))
        .stdin(Stdio::null())
        .stdout(Stdio::from(file))
        .stderr(Stdio::null())
        .spawn()
}
