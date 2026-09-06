//! A tool-less completion, used when a seat has to speak and must not work.
//!
//! The agent CLI cannot be told to stop calling tools — asked politely, in as
//! many words, it answers by running eight more commands and hitting the
//! deadline again. So the wrap-up does not go through the CLI at all: it is one
//! plain chat completion, where there is no tool to call. Same router, same
//! model, over `curl` for the same reason [`super::memory`] uses it.

use std::{
    io::Write,
    process::{Command, Stdio},
    time::Duration,
};

/// One model, reachable over an OpenAI-shaped `/v1/chat/completions`.
pub(crate) struct Chat {
    base: String,
    key: String,
    model: String,
    timeout: Duration,
}

impl Chat {
    /// Build a client against a router root, without a trailing slash.
    pub(crate) fn new(base: &str, key: &str, model: &str, timeout: Duration) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            key: key.to_string(),
            model: model.to_string(),
            timeout,
        }
    }

    /// Complete one prompt, returning the assistant text or the empty string.
    pub(crate) fn complete(&self, prompt: &str) -> String {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": prompt }],
            "max_tokens": 1200,
        });
        let Some(response) = self.post("/v1/chat/completions", &body) else {
            return String::new();
        };
        serde_json::from_str::<serde_json::Value>(&response)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/choices/0/message/content")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default()
    }

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
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).to_string())
    }
}
