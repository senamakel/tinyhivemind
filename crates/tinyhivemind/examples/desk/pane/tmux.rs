//! The window the room is watched in: one pane per seat, stacked.
//!
//! Nothing here is load-bearing for the desk — a run with no tmux on the box
//! still works, it is simply not watchable. That is the reason the layout is
//! built as data and then run: what a reader wants to check about a window is
//! the commands it would issue, and those are testable without a terminal.

use std::process::{Command, Stdio};

use crate::BoxError;

/// One seat's pane: which seat, and the server its terminal attaches to.
pub(crate) struct Attach {
    /// The seat id, used as the pane title.
    pub(crate) seat: String,
    /// The base URL of that seat's `opencode serve`.
    pub(crate) base: String,
}

/// The `tmux` invocations that build the window, in order.
///
/// Three seats become three stacked panes — the 3x1 the room is read in —
/// because a desk is a column of messages and a column of panes is the same
/// shape. `even-vertical` is applied after the splits rather than relied on
/// during them: splitting three ways without it leaves the last pane half the
/// height of the first.
///
/// Pane order is the seat order, and it has to be: the host reads a pane back
/// by index to check the terminal in it is listening, so a window whose panes
/// are in a different order than its seats types into one seat and watches
/// another.
pub(crate) fn layout(
    session: &str,
    workspace: &str,
    config: Option<&str>,
    seats: &[Attach],
) -> Vec<Vec<String>> {
    let mut plan = Vec::new();
    for (index, seat) in seats.iter().enumerate() {
        let mut argv: Vec<String> = if index == 0 {
            vec![
                "new-session".into(),
                "-d".into(),
                "-s".into(),
                session.into(),
                "-n".into(),
                "desk".into(),
            ]
        } else {
            vec![
                "split-window".into(),
                "-d".into(),
                "-t".into(),
                format!("{session}:desk"),
            ]
        };
        argv.push("-c".into());
        argv.push(workspace.into());
        if index > 0 {
            argv.push("-v".into());
        }
        if let Some(config) = config {
            // Through tmux's environment rather than the command line: the
            // provider block carries the router's key, and an argument is
            // visible in every `ps` on the box while an environment entry is
            // not.
            argv.push("-e".into());
            argv.push(format!("OPENCODE_CONFIG_CONTENT={config}"));
        }
        argv.push(format!("opencode attach {} --dir {workspace}", seat.base));
        plan.push(argv);
    }
    plan.push(vec![
        "select-layout".into(),
        "-t".into(),
        format!("{session}:desk"),
        "even-vertical".into(),
    ]);
    plan.push(vec![
        "set-option".into(),
        "-t".into(),
        session.into(),
        "pane-border-status".into(),
        "top".into(),
    ]);
    plan.push(vec![
        "set-option".into(),
        "-t".into(),
        session.into(),
        "pane-border-format".into(),
        " #{@seat} ".into(),
    ]);
    for (index, seat) in seats.iter().enumerate() {
        // A pane-scoped user option rather than `select-pane -T`: the agent
        // terminal sets its own pane title the moment it draws, so a title
        // the host set is gone by the time anybody looks at it.
        plan.push(vec![
            "set-option".into(),
            "-p".into(),
            "-t".into(),
            format!("{session}:desk.{index}"),
            "@seat".into(),
            format!("@{}", seat.seat),
        ]);
    }
    plan
}

/// Build the window, replacing any session of the same name.
///
/// # Errors
///
/// Returns the first `tmux` invocation that failed, with its stderr. A missing
/// `tmux` is reported the same way: the caller decides whether a run without a
/// window is still worth having.
pub(crate) fn build(
    session: &str,
    workspace: &str,
    config: Option<&str>,
    seats: &[Attach],
) -> Result<(), BoxError> {
    kill(session);
    for argv in layout(session, workspace, config, seats) {
        let output = Command::new("tmux")
            .args(&argv)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| {
                format!(
                    "tmux {}: {error}",
                    argv.first().cloned().unwrap_or_default()
                )
            })?;
        if !output.status.success() {
            return Err(format!(
                "tmux {} failed: {}",
                argv.first().cloned().unwrap_or_default(),
                String::from_utf8_lossy(&output.stderr).trim()
            )
            .into());
        }
    }
    Ok(())
}

/// Read back what a pane is currently showing.
///
/// This is how the host checks that a terminal is listening rather than
/// merely running: a server answers its health endpoint some seconds before
/// the terminal attached to it will accept a prompt, and a prompt submitted
/// into that gap is silently lost.
pub(crate) fn capture(session: &str, index: usize) -> Option<String> {
    let output = Command::new("tmux")
        .args([
            "capture-pane",
            "-p",
            "-t",
            &format!("{session}:desk.{index}"),
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

/// Tear down a session, ignoring one that is not there.
pub(crate) fn kill(session: &str) {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
