//! Running one seat's turn as one child process.
//!
//! One message, one turn: the library authorizes exactly one speaker per step,
//! and this module is what a speaker costs — a single `opencode run` in the
//! desk's shared workspace, so the seat can read files, write code, and run it.
//! Nothing here decides *who* speaks.

use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// What one turn produced.
#[derive(Clone, Debug, Default)]
pub struct TurnOutput {
    /// The text the seat wants posted to the room.
    pub message: String,
    /// Total tokens the provider reported across the turn's steps.
    pub tokens: u64,
    /// Wall-clock cost of the process.
    pub elapsed: Duration,
    /// Names of the tools the seat actually invoked, in order.
    pub tools: Vec<String>,
}

/// A configured agent CLI: one process per turn.
pub struct AgentRunner {
    program: String,
    args: Vec<String>,
    workspace: String,
    config: Option<String>,
    timeout: Duration,
}

impl AgentRunner {
    /// Build a runner from a command line whose final argument is the prompt.
    pub fn new(command: &str, workspace: &str, config: Option<String>, timeout: Duration) -> Self {
        let mut parts = command.split_whitespace().map(str::to_string);
        let program = parts.next().unwrap_or_else(|| "opencode".to_string());
        Self {
            program,
            args: parts.collect(),
            workspace: workspace.to_string(),
            config,
            timeout,
        }
    }

    /// Run one turn.
    ///
    /// # Errors
    ///
    /// Returns a spawn or wait failure. A model that answers nothing is not an
    /// error here; it is an empty message the caller decides what to do with.
    pub fn run(&self, prompt: &str) -> Result<TurnOutput, Box<dyn std::error::Error + Send + Sync>> {
        let started = Instant::now();
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .arg(prompt)
            .current_dir(&self.workspace)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(config) = &self.config {
            command.env("OPENCODE_CONFIG_CONTENT", config);
        }
        let child = command.spawn()?;
        let output = wait_with_timeout(child, self.timeout)?;
        let mut turn = parse_events(&String::from_utf8_lossy(&output));
        turn.elapsed = started.elapsed();
        Ok(turn)
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    let stdout = child.stdout.take().ok_or("child has no stdout")?;
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut buffer = Vec::new();
        let mut stdout = stdout;
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                break;
            }
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    }
    Ok(reader.join().unwrap_or_default())
}

/// Fold one `--format json` event stream into a turn.
///
/// Text parts are concatenated in arrival order; a `<<<POST ... POST>>>` block
/// wins over everything around it, because a model narrating its own reasoning
/// into the shared transcript is noise every other seat then has to read.
fn parse_events(stdout: &str) -> TurnOutput {
    let mut text = String::new();
    let mut turn = TurnOutput::default();
    for line in stdout.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match event.get("type").and_then(serde_json::Value::as_str) {
            Some("text") => {
                if let Some(part) = event.pointer("/part/text").and_then(serde_json::Value::as_str) {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(part);
                }
            }
            Some("tool") | Some("tool_use") => {
                if let Some(name) = event
                    .pointer("/part/tool")
                    .and_then(serde_json::Value::as_str)
                {
                    turn.tools.push(name.to_string());
                }
            }
            Some("step_finish") => {
                if let Some(total) = event
                    .pointer("/part/tokens/total")
                    .and_then(serde_json::Value::as_u64)
                {
                    turn.tokens = turn.tokens.max(total);
                }
            }
            _ => {}
        }
    }
    turn.message = extract_post(&text);
    turn
}

/// Pull the room message out of a turn's raw text.
fn extract_post(text: &str) -> String {
    if let Some(start) = text.rfind("<<<POST") {
        let after = &text[start + "<<<POST".len()..];
        if let Some(end) = after.find("POST>>>") {
            return after[..end].trim().to_string();
        }
        return after.trim().to_string();
    }
    text.trim().to_string()
}

#[cfg(test)]
mod test {
    use super::extract_post;

    #[test]
    fn takes_the_marked_block_over_surrounding_narration() {
        let raw = "thinking out loud\n<<<POST\n@checker ready\nPOST>>>\ndone";
        assert_eq!(extract_post(raw), "@checker ready");
    }

    #[test]
    fn falls_back_to_the_whole_text_when_unmarked() {
        assert_eq!(extract_post("  plain answer \n"), "plain answer");
    }
}
