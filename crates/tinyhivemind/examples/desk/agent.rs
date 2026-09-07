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
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

/// How long a turn may produce nothing at all before it is treated as stalled.
///
/// A seat that is working emits an event every few seconds — a step, a tool
/// call, a token. Silence for minutes means the agent CLI is waiting on a
/// request that will never come back: one intermittent upstream failure ends
/// its progress and it neither retries nor exits. Without this the only thing
/// that notices is the turn deadline, so a provider blip costs a whole turn.
const STALL_AFTER: Duration = Duration::from_secs(240);

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
    /// Whether it was killed for going quiet long before its deadline.
    pub(crate) stalled: bool,
    /// The agent CLI's own session id, so this seat can resume its own work.
    pub(crate) session: Option<String>,
    /// What the seat actually ran and saw, so a wrap-up summarizes evidence
    /// rather than inventing it.
    pub(crate) work_log: String,
    /// A provider or CLI failure reported inside the event stream.
    ///
    /// A 402 from a fallback provider arrives here, not as a nonzero exit: the
    /// process runs its full budget and prints one `error` event, so a turn
    /// that lost its credit is indistinguishable from a model that would not
    /// speak unless the host reads this.
    pub(crate) error: Option<String>,
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
        let mut child = command.spawn()?;
        // Keep stderr. A turn whose event stream is zero bytes said nothing and
        // explained nothing; whatever the CLI complained about went here, and
        // discarding it is how a crash reads as a quiet model.
        let stderr = child.stderr.take();
        let errors = std::thread::spawn(move || {
            use std::io::Read as _;
            let mut buffer = String::new();
            if let Some(mut stderr) = stderr {
                let _ = stderr.read_to_string(&mut buffer);
            }
            buffer
        });
        let (output, timed_out, stalled) = wait_with_timeout(child, timeout)?;
        let raw = String::from_utf8_lossy(&output);
        let complaints = errors.join().unwrap_or_default();
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
            if !complaints.trim().is_empty()
                && let Ok(mut file) = fs::File::create(dir.join(format!("{label}.stderr.txt")))
            {
                let _ = file.write_all(complaints.as_bytes());
            }
        }
        let mut turn = parse_events(&raw);
        turn.elapsed = started.elapsed();
        turn.timed_out = timed_out;
        turn.stalled = stalled;
        if turn.error.is_none() && raw.trim().is_empty() && !complaints.trim().is_empty() {
            turn.error = Some(complaints.trim().lines().last().unwrap_or("").to_string());
        }
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
) -> Result<(Vec<u8>, bool, bool), Box<dyn std::error::Error + Send + Sync>> {
    let stdout = child.stdout.take().ok_or("child has no stdout")?;
    // The buffer and the time it last grew, shared with the reader: a turn is
    // alive because it is still saying something, not because it is still
    // running.
    let seen = Arc::new(Mutex::new((Vec::new(), Instant::now())));
    let writer = Arc::clone(&seen);
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut stdout = stdout;
        let mut chunk = [0_u8; 8192];
        while let Ok(read) = stdout.read(&mut chunk) {
            if read == 0 {
                break;
            }
            let mut held = writer.lock().unwrap_or_else(PoisonError::into_inner);
            held.0.extend_from_slice(&chunk[..read]);
            held.1 = Instant::now();
        }
    });
    let deadline = Instant::now() + timeout;
    let mut timed_out = false;
    let mut stalled = false;
    loop {
        match child.try_wait()? {
            Some(_) => break,
            None => {
                let quiet = seen
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .1
                    .elapsed();
                if Instant::now() >= deadline {
                    timed_out = true;
                    let _ = child.kill();
                    break;
                }
                if quiet >= STALL_AFTER {
                    stalled = true;
                    let _ = child.kill();
                    break;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        }
    }
    let _ = reader.join();
    let buffer = seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .0
        .clone();
    Ok((buffer, timed_out, stalled))
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
