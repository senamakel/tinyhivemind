//! Running one seat's turn as one child process.
//!
//! One message, one turn: the library authorizes exactly one speaker per step,
//! and this module is what a speaker costs — a single `opencode run` in the
//! desk's shared workspace, so the seat can read files, write code, and run it.
//! Nothing here decides *who* speaks.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// What one turn produced.
#[derive(Clone, Debug, Default)]
pub(crate) struct TurnOutput {
    /// The text the seat wants posted to the room.
    pub(crate) message: String,
    /// Total tokens the provider reported across the turn's steps.
    pub(crate) tokens: u64,
    /// Wall-clock cost of the process.
    pub(crate) elapsed: Duration,
    /// Names of the tools the seat actually invoked, in order.
    pub(crate) tools: Vec<String>,
    /// Whether the process was killed at its deadline rather than finishing.
    pub(crate) timed_out: bool,
}

/// A configured agent CLI: one process per turn.
pub(crate) struct AgentRunner {
    program: String,
    args: Vec<String>,
    workspace: String,
    config: Option<String>,
    timeout: Duration,
    raw_dir: Option<PathBuf>,
}

impl AgentRunner {
    /// Build a runner from a command line whose final argument is the prompt.
    pub(crate) fn new(
        command: &str,
        workspace: &str,
        config: Option<String>,
        timeout: Duration,
        raw_dir: Option<PathBuf>,
    ) -> Self {
        let mut parts = command.split_whitespace().map(str::to_string);
        let program = parts.next().unwrap_or_else(|| "opencode".to_string());
        Self {
            program,
            args: parts.collect(),
            workspace: workspace.to_string(),
            config,
            timeout,
            raw_dir,
        }
    }

    /// Run one turn.
    ///
    /// # Errors
    ///
    /// Returns a spawn or wait failure. A model that answers nothing is not an
    /// error here; it is an empty message the caller decides what to do with.
    pub(crate) fn run(
        &self,
        prompt: &str,
        label: &str,
        timeout: Duration,
    ) -> Result<TurnOutput, Box<dyn std::error::Error + Send + Sync>> {
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
        let (output, timed_out) = wait_with_timeout(child, timeout)?;
        let raw = String::from_utf8_lossy(&output);
        // Keep the whole event stream. A turn that produced nothing postable is
        // exactly the turn whose transcript someone will want to read.
        if let Some(dir) = &self.raw_dir {
            let _ = fs::create_dir_all(dir);
            if let Ok(mut file) = fs::File::create(dir.join(format!("{label}.jsonl"))) {
                let _ = file.write_all(raw.as_bytes());
            }
            if let Ok(mut file) = fs::File::create(dir.join(format!("{label}.prompt.txt"))) {
                let _ = file.write_all(prompt.as_bytes());
            }
        }
        let mut turn = parse_events(&raw);
        turn.elapsed = started.elapsed();
        turn.timed_out = timed_out;
        Ok(turn)
    }

    /// The runner's configured per-turn deadline.
    pub(crate) const fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Where raw turn streams are kept, if anywhere.
    pub(crate) fn raw_dir(&self) -> Option<&Path> {
        self.raw_dir.as_deref()
    }
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Result<(Vec<u8>, bool), Box<dyn std::error::Error + Send + Sync>> {
    let stdout = child.stdout.take().ok_or("child has no stdout")?;
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut buffer = Vec::new();
        let mut stdout = stdout;
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None if Instant::now() >= deadline => {
                timed_out = true;
                let _ = child.kill();
                break;
            }
            None => std::thread::sleep(Duration::from_millis(200)),
        }
    }
    Ok((reader.join().unwrap_or_default(), timed_out))
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
