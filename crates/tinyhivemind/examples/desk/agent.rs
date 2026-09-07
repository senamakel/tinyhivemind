//! Running one seat's turn as one child process.
//!
//! One message, one turn: the library authorizes exactly one speaker per step,
//! and this module is what a speaker costs — a single `opencode run` in the
//! desk's shared workspace, so the seat can read files, write code, and run it.
//! Nothing here decides *who* speaks.

use std::{
    fs,
    io::Write,
    path::PathBuf,
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
    /// The agent CLI's own session id, so this seat can resume its own work.
    pub(crate) session: Option<String>,
    /// What the seat actually ran and saw, so a wrap-up summarizes evidence
    /// rather than inventing it.
    pub(crate) work_log: String,
    /// Whether the seat actually marked a message for the room.
    ///
    /// Unmarked trailing text is narration — "Now I have the full picture, let
    /// me write the solver" — and posting it wastes a turn and tells the room
    /// nothing. A turn that did not mark a post has not spoken.
    pub(crate) posted: bool,
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
        session: Option<&str>,
    ) -> Result<TurnOutput, Box<dyn std::error::Error + Send + Sync>> {
        let started = Instant::now();
        let mut command = Command::new(&self.program);
        command.args(&self.args);
        // A seat that resumes its own CLI session keeps the working context it
        // built last turn — its scratch reasoning, its file reads — which is
        // what a persistent agent has and a fresh process does not.
        if let Some(session) = session {
            command.args(["--session", session]);
        }
        command
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
        if turn.session.is_none()
            && let Some(id) = event.get("sessionID").and_then(serde_json::Value::as_str)
        {
            turn.session = Some(id.to_string());
        }
        match event.get("type").and_then(serde_json::Value::as_str) {
            Some("text") => {
                if let Some(part) = event
                    .pointer("/part/text")
                    .and_then(serde_json::Value::as_str)
                {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(part);
                }
            }
            Some("tool" | "tool_use") => {
                if let Some(name) = event
                    .pointer("/part/tool")
                    .and_then(serde_json::Value::as_str)
                {
                    turn.tools.push(name.to_string());
                }
                let input = event
                    .pointer("/part/state/input")
                    .map(serde_json::Value::to_string)
                    .unwrap_or_default();
                let result = event
                    .pointer("/part/state/output")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                turn.work_log.push_str("\n$ ");
                turn.work_log.push_str(&truncate(&input, 1200));
                turn.work_log.push('\n');
                turn.work_log.push_str(&truncate(result, 1200));
                turn.work_log.push('\n');
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
    turn.posted = text.contains("<<<POST");
    turn.message = extract_post(&text);
    turn
}

/// Keep the head of a long string, marking what was dropped.
fn truncate(text: &str, limit: usize) -> String {
    let mut end = limit.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == text.len() {
        return text.to_string();
    }
    format!("{}… [{} bytes elided]", &text[..end], text.len() - end)
}

/// Pull the room message out of a turn's raw text.
pub(crate) fn extract_post(text: &str) -> String {
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

/// Trim a turn's work log to the tail a wrap-up can actually read.
pub(crate) fn truncate_work_log(log: &str) -> String {
    const LIMIT: usize = 12_000;
    if log.len() <= LIMIT {
        return log.to_string();
    }
    let mut start = log.len() - LIMIT;
    while start < log.len() && !log.is_char_boundary(start) {
        start += 1;
    }
    format!("[earlier steps elided]\n{}", &log[start..])
}
