//! One long-lived agent terminal per seat, in a tmux window, driven over HTTP.
//!
//! [`crate::agent`] runs a seat as one child process and reads its stdout.
//! That is the cheapest thing that works and it is invisible: the only trace a
//! turn leaves while it runs is a line of telemetry after it ends. This module
//! is the same turn made watchable. Each seat gets its own `opencode serve`
//! and its own real terminal attached to it in a tmux pane, and the host takes
//! a turn the way a person would — put the prompt in the box, press enter —
//! through the server's own `/tui` endpoints.
//!
//! Three things make that honest rather than a screenshot:
//!
//! - **The pane is the session, not a mirror of it.** There is no second
//!   channel. The prompt the host submits is the prompt the terminal shows
//!   because it is the same box, and the tool calls scrolling past are the
//!   ones the desk is about to be told happened.
//! - **The desk still owns the turn.** The server publishes; the host decides.
//!   A seat speaks by calling `desk_post` exactly as before, the outbox is
//!   still drained per turn, and `session.idle` is where a turn *ends*, not
//!   where a message becomes true.
//! - **A seat is still one turn's worth of attention.** The terminal is
//!   long-lived; whether the conversation inside it is depends on what the
//!   desk hands [`PaneDesk::run`] as `session`, exactly as it does for a
//!   child process.
//!
//! The cost is that a seat's context now accumulates across turns in a way a
//! fresh process's never did, which is the failure `README.md` records as
//! tried and turned off. [`PaneConfig::compact_at`] is the guard: when a
//! session's reported tokens cross it, the host asks the server to summarize
//! before the next turn goes in.

mod events;
mod http;
mod tmux;

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use crate::{BoxError, agent::TurnOutput};

/// How long a seat's server is given to come up before the run is abandoned.
const BOOT_TIMEOUT: Duration = Duration::from_secs(60);

/// How often the host looks at a pane's event feed while a turn runs.
///
/// The feed is a file `curl` appends to, so this is a read of a few kilobytes
/// against a page cache, not a request. Half a second keeps the host's own
/// telemetry roughly live without spending a core on it.
const POLL: Duration = Duration::from_millis(500);

/// How long the calls that drive a terminal may take.
///
/// These are localhost `POST`s that return as soon as the TUI has the text;
/// none of them waits on a model.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);

/// What a watched desk needs to build itself.
pub(crate) struct PaneConfig {
    /// The tmux session name to build, replacing one of the same name.
    pub(crate) session: String,
    /// The directory every seat reads, writes, and runs code in.
    pub(crate) workspace: PathBuf,
    /// `OPENCODE_CONFIG_CONTENT` for the servers: provider block and the
    /// desk's own MCP block, exactly what the child process would have been
    /// given.
    pub(crate) config: Option<String>,
    /// The first port to use; seats take one each, in order.
    pub(crate) base_port: u16,
    /// The seat ids, in pane order.
    pub(crate) seats: Vec<String>,
    /// Tokens a seat's session may reach before it is summarized.
    pub(crate) compact_at: u64,
    /// Where each turn's raw event feed and prompt are filed.
    pub(crate) raw_dir: Option<PathBuf>,
}

/// One seat's terminal: a server, a feed of its events, and where it is read.
struct Pane {
    /// The seat this pane belongs to.
    seat: String,
    /// The root URL of the seat's server.
    base: String,
    /// The file `curl` appends this server's event stream to.
    feed: PathBuf,
    /// The server process.
    server: Child,
    /// The `curl` following its events.
    follower: Child,
}

/// A tmux window of seats, each a live agent terminal the host can drive.
pub(crate) struct PaneDesk {
    /// The tmux session holding the panes.
    session: String,
    /// One pane per seat, in the order they were declared.
    panes: Vec<Pane>,
    /// Tokens a seat's session may reach before it is summarized.
    compact_at: u64,
    /// Where each turn's raw event feed and prompt are filed.
    raw_dir: Option<PathBuf>,
}

impl PaneDesk {
    /// Start a server per seat, follow their events, and build the window.
    ///
    /// # Errors
    ///
    /// Returns a spawn failure, a server that never answered within
    /// [`BOOT_TIMEOUT`], or a `tmux` invocation that failed.
    pub(crate) fn start(config: &PaneConfig) -> Result<Self, BoxError> {
        let logs = config.workspace.join(".desk/panes");
        fs::create_dir_all(&logs)?;
        let mut panes = Vec::new();
        for (index, seat) in config.seats.iter().enumerate() {
            let port = config.base_port + u16::try_from(index)?;
            let base = format!("http://127.0.0.1:{port}");
            let server = serve(config, port, &logs.join(format!("{seat}.server.log")))?;
            let feed = logs.join(format!("{seat}.events.jsonl"));
            let _ = fs::remove_file(&feed);
            wait_until_up(&base)?;
            let follower = http::follow_events(&base, &feed)?;
            println!("   pane {index}: @{seat} on {base}");
            panes.push(Pane {
                seat: seat.clone(),
                base,
                feed,
                server,
                follower,
            });
        }
        let attachments: Vec<tmux::Attach> = panes
            .iter()
            .map(|pane| tmux::Attach {
                seat: pane.seat.clone(),
                base: pane.base.clone(),
            })
            .collect();
        tmux::build(
            &config.session,
            &config.workspace.to_string_lossy(),
            config.config.as_deref(),
            &attachments,
        )?;
        println!("   watch it with: tmux attach -t {}", config.session);
        Ok(Self {
            session: config.session.clone(),
            panes,
            compact_at: config.compact_at,
            raw_dir: config.raw_dir.clone(),
        })
    }

    /// Take one turn in a seat's terminal.
    ///
    /// `session` is the same contract [`crate::agent::AgentRunner::run`] uses:
    /// `Some` keeps the conversation the terminal is already in, `None` starts
    /// a fresh one in the same pane.
    ///
    /// # Errors
    ///
    /// Returns an error only when the seat has no pane, or its terminal
    /// refused the prompt. A model that says nothing is a `TurnOutput` that
    /// says nothing, which is what the recovery ladder above exists for.
    pub(crate) fn run(
        &self,
        seat: &str,
        prompt: &str,
        label: &str,
        timeout: Duration,
        session: Option<&str>,
    ) -> Result<TurnOutput, BoxError> {
        let pane = self
            .panes
            .iter()
            .find(|pane| pane.seat == seat)
            .ok_or_else(|| format!("no pane for seat {seat}"))?;
        if session.is_none() {
            // A fresh conversation in the same terminal. The pane clears, so
            // the window shows what the desk believes: this seat is starting
            // over, not continuing.
            http::post(
                &pane.base,
                "/tui/execute-command",
                &serde_json::json!({ "command": "session.new" }).to_string(),
                CONTROL_TIMEOUT,
            );
            std::thread::sleep(Duration::from_secs(1));
        }
        let started = Instant::now();
        let from = fs::metadata(&pane.feed).map(|meta| meta.len()).unwrap_or(0);
        submit(&pane.base, prompt)?;
        let mut turn = self.watch(pane, from, started, timeout);
        turn.elapsed = started.elapsed();
        self.file_raw(pane, label, prompt, from);
        if let Some(id) = turn.session.clone()
            && turn.tokens > self.compact_at
        {
            println!(
                "   pane at {} tokens; summarizing before the next turn",
                turn.tokens
            );
            http::post(
                &pane.base,
                &format!("/session/{id}/summarize"),
                "{}",
                CONTROL_TIMEOUT,
            );
        }
        Ok(turn)
    }

    /// Read the pane's feed until the turn ends, times out, or goes quiet.
    fn watch(&self, pane: &Pane, from: u64, started: Instant, timeout: Duration) -> TurnOutput {
        let mut watch = events::Watch::default();
        let mut read = from;
        let mut last_growth = Instant::now();
        loop {
            std::thread::sleep(POLL);
            let (lines, at) = tail(&pane.feed, read);
            if at > read {
                read = at;
                last_growth = Instant::now();
            }
            for line in lines.lines() {
                watch.absorb(line);
            }
            if watch.done() {
                return watch.finish();
            }
            if started.elapsed() >= timeout {
                interrupt(pane, watch.session());
                let mut turn = watch.finish();
                turn.timed_out = true;
                return turn;
            }
            if last_growth.elapsed() >= crate::agent::STALL_AFTER {
                interrupt(pane, watch.session());
                let mut turn = watch.finish();
                turn.stalled = true;
                return turn;
            }
        }
    }

    /// Keep the turn's own slice of the feed, and the prompt that caused it.
    fn file_raw(&self, pane: &Pane, label: &str, prompt: &str, from: u64) {
        let Some(dir) = &self.raw_dir else {
            return;
        };
        let _ = fs::create_dir_all(dir);
        let (lines, _) = tail(&pane.feed, from);
        let _ = fs::write(dir.join(format!("{label}.jsonl")), lines);
        let _ = fs::write(dir.join(format!("{label}.prompt.txt")), prompt);
    }
}

impl Drop for PaneDesk {
    fn drop(&mut self) {
        for pane in &mut self.panes {
            let _ = pane.follower.kill();
            let _ = pane.server.kill();
        }
        // The window is left standing on purpose. Its panes hold the last
        // thing each seat did, which is the reason to have watched at all;
        // `tmux kill-session -t <name>` is the one line that reclaims it.
        println!(
            "   panes left up for reading: tmux attach -t {}",
            self.session
        );
    }
}

/// Stop a turn that will not stop itself.
///
/// The pane is not closed and the terminal is not restarted: the seat's work
/// is in the shared workspace either way, and the next rung of the ladder
/// wants the same terminal to land it.
fn interrupt(pane: &Pane, session: Option<&str>) {
    if let Some(id) = session {
        http::post(
            &pane.base,
            &format!("/session/{id}/abort"),
            "{}",
            CONTROL_TIMEOUT,
        );
    }
}

/// Start one seat's server in the shared workspace.
fn serve(config: &PaneConfig, port: u16, log: &Path) -> Result<Child, BoxError> {
    let file = fs::File::create(log)?;
    let errors = file.try_clone()?;
    let mut command = Command::new("opencode");
    command
        .args([
            "serve",
            "--port",
            &port.to_string(),
            "--hostname",
            "127.0.0.1",
        ])
        .current_dir(&config.workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(errors));
    if let Some(content) = &config.config {
        command.env("OPENCODE_CONFIG_CONTENT", content);
    }
    Ok(command.spawn()?)
}

/// Block until a server answers, or give up.
fn wait_until_up(base: &str) -> Result<(), BoxError> {
    let deadline = Instant::now() + BOOT_TIMEOUT;
    while Instant::now() < deadline {
        if http::get(base, "/global/health", Duration::from_secs(5)).is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(400));
    }
    Err(format!("{base} did not come up within {BOOT_TIMEOUT:?}").into())
}

/// Put the prompt in the terminal's box and press enter.
fn submit(base: &str, prompt: &str) -> Result<(), BoxError> {
    http::post(base, "/tui/clear-prompt", "{}", CONTROL_TIMEOUT);
    let body = serde_json::json!({ "text": prompt }).to_string();
    http::post(base, "/tui/append-prompt", &body, CONTROL_TIMEOUT)
        .ok_or_else(|| format!("{base} would not take the prompt"))?;
    http::post(base, "/tui/submit-prompt", "{}", CONTROL_TIMEOUT)
        .ok_or_else(|| format!("{base} would not submit the prompt"))?;
    Ok(())
}

/// Read a file from a byte offset, returning what was there and where it ends.
///
/// A partial last line is left for the next read: an event frame cut in half
/// is not a frame, and half of one arriving twice is worse than late.
fn tail(path: &Path, from: u64) -> (String, u64) {
    let Ok(text) = fs::read_to_string(path) else {
        return (String::new(), from);
    };
    let bytes = text.as_bytes();
    let mut start = usize::try_from(from).unwrap_or(0).min(bytes.len());
    while start < bytes.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    let rest = &text[start..];
    let Some(end) = rest.rfind('\n') else {
        return (String::new(), from);
    };
    let whole = &rest[..=end];
    (whole.to_string(), start as u64 + whole.len() as u64)
}

#[cfg(test)]
mod test;
