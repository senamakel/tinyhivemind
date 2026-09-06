//! The desk's memory, which is a host concern and lives outside the library.
//!
//! Two calls: recall a bounded context block before a seat speaks, and capture
//! what it said afterwards. Both go over the `curl` binary rather than an HTTP
//! crate, for the same reason the deliberation benchmark does — the boundary
//! this workspace enforces is easier to keep when nothing in it can reach the
//! network by accident.

use std::{
    io::Write,
    process::{Command, Stdio},
    time::Duration,
};

/// A CortexDB scope pair: a durable library and this run's session.
pub(crate) struct Memory {
    base: String,
    key: String,
    library_scope: String,
    session_scope: String,
    timeout: Duration,
}

impl Memory {
    /// Build a client. `base` is the service root, without a trailing slash.
    pub(crate) fn new(
        base: &str,
        key: &str,
        library_scope: &str,
        session_scope: &str,
        timeout: Duration,
    ) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            key: key.to_string(),
            library_scope: library_scope.to_string(),
            session_scope: session_scope.to_string(),
            timeout,
        }
    }

    /// Ask both scopes what they hold about `query`, newest desk memory last.
    ///
    /// A memory that cannot be reached is not a turn failure: the seat speaks
    /// without it and the caller sees the empty string.
    pub(crate) fn recall(&self, query: &str) -> String {
        let mut blocks = Vec::new();
        for scope in [&self.library_scope, &self.session_scope] {
            let body = serde_json::json!({
                "scope": scope,
                "query": query,
                "view": "holistic",
            });
            let Some(response) = self.post("/v1/recall", &body) else {
                continue;
            };
            let Ok(value) = serde_json::from_str::<serde_json::Value>(&response) else {
                continue;
            };
            let block = value
                .get("context_block")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .trim();
            if !block.is_empty() {
                blocks.push(block.to_string());
            }
        }
        blocks.join("\n\n")
    }

    /// File one room message in this run's session scope.
    pub(crate) fn capture(&self, seat: &str, sequence: u64, text: &str) {
        let body = serde_json::json!({
            "scope": self.session_scope,
            "modality": "message",
            "content": { "kind": "text", "text": format!("{seat}: {text}") },
            "context": { "labels": [format!("seat:{seat}")] },
            "idempotency_key": format!("{}-{sequence}", self.session_scope_slug()),
        });
        let _ = self.post("/v1/experience", &body);
    }

    fn session_scope_slug(&self) -> String {
        self.session_scope
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect()
    }

    /// POST one JSON body, handing `curl` the whole request over stdin so no
    /// key ever appears in a process argument.
    fn post(&self, path: &str, body: &serde_json::Value) -> Option<String> {
        let mut child = Command::new("curl")
            .args([
                "-sS",
                "--max-time",
                &self.timeout.as_secs().to_string(),
                "--config",
                "-",
                &format!("{}{path}", self.base),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        {
            let stdin = child.stdin.as_mut()?;
            let config = format!(
                "header = \"Authorization: Bearer {}\"\nheader = \"content-type: application/json\"\ndata-binary = {}\n",
                self.key,
                serde_json::Value::String(body.to_string()),
            );
            stdin.write_all(config.as_bytes()).ok()?;
        }
        let output = child.wait_with_output().ok()?;
        if !output.status.success() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).to_string())
    }
}
