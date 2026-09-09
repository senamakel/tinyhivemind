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
        " #{pane_title} ".into(),
    ]);
    for (index, seat) in seats.iter().enumerate() {
        plan.push(vec![
            "select-pane".into(),
            "-t".into(),
            format!("{session}:desk.{index}"),
            "-T".into(),
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

/// Tear down a session, ignoring one that is not there.
pub(crate) fn kill(session: &str) {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
