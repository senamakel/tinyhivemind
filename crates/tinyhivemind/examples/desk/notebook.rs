//! The notebook a seat carries between turns, and what a turn left behind.
//!
//! A seat's process ends when it posts, so nothing in it survives. The
//! notebook is the one thing the host hands a seat back from its own last
//! turn: a file the seat rewrites, read verbatim at the top of the next
//! prompt. This module owns where it lives, how much of it comes back, and
//! which of a turn's written files the room is told about.

use std::{fs, path::Path};

/// How much of its own notebook a seat is handed back.
///
/// The notebook is the fold a seat carries between turns — what it
/// established, what it is mid-way through, what it would tell itself next —
/// read verbatim at the top of its next turn. It is what a continuously
/// contexted seat has for free and what a fresh process per turn lacks. The
/// budget is stated to the seat and enforced on read, keeping the tail: a seat
/// that rewrites has no age gradient and loses nothing that fits, and a seat
/// that appended instead has its oldest material fall off first.
pub(crate) const NOTEBOOK_CHARS: usize = 6000;

/// Where a seat's notebook lives, under the shared workspace.
pub(crate) const NOTEBOOK_DIR: &str = "notebooks";

/// Read back the tail of a seat's notebook, within budget.
///
/// `None` when the seat has not written one. The file is never rewritten by
/// the host: an overrun is truncated on read and reported in the first line,
/// so the seat sees its own overrun and the file stays what the seat wrote.
pub(crate) fn read_notebook(workspace: &Path, seat_id: &str) -> Option<String> {
    let path = workspace.join(NOTEBOOK_DIR).join(format!("{seat_id}.md"));
    let text = fs::read_to_string(path).ok()?;
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let total = text.chars().count();
    if total <= NOTEBOOK_CHARS {
        return Some(text.to_string());
    }
    let dropped = total - NOTEBOOK_CHARS;
    let start = text
        .char_indices()
        .nth(dropped)
        .map_or(text.len(), |(at, _)| at);
    Some(format!(
        "(your notebook is {total} characters against a budget of {NOTEBOOK_CHARS}; the first \
         {dropped} were dropped — rewrite it shorter)\n{}",
        &text[start..]
    ))
}

/// The base names a turn wrote, with the seat's own notebook left out.
pub(crate) fn files_written(paths: &[String], seat_id: &str) -> Vec<String> {
    let own = format!("{NOTEBOOK_DIR}/{seat_id}.md");
    paths
        .iter()
        .filter(|path| !path.ends_with(&own))
        .filter_map(|path| Path::new(path).file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect()
}
