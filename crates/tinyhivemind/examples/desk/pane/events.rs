//! Folding a pane's server-sent event stream into one turn's output.
//!
//! The shape is the one [`crate::agent::parse_events`] already knows — a part
//! carries `tool`, `state.input`, `state.output` — wrapped in a different
//! envelope. `opencode run --format json` prints `{"type":"tool_use","part":…}`
//! straight to stdout; a server publishes `{"type":"message.part.updated",
//! "properties":{"part":…}}` to every subscriber. So this module unwraps and
//! the folding rules stay one set of rules.
//!
//! One thing genuinely differs, and it is why a part cannot simply be pushed
//! as it arrives: a *stream* emits each part once, finished, while a *server*
//! republishes the same part id as it moves `pending → running → completed`.
//! Keeping the last state per id, in first-seen order, is what turns twenty
//! updates back into the three tool calls a seat actually made.

use std::collections::HashMap;

use serde_json::Value;

use crate::agent::{TurnOutput, extract_post};

/// What one turn's events amount to so far.
#[derive(Default)]
pub(crate) struct Watch {
    /// The session the turn is running in, learned from its first event.
    session: Option<String>,
    /// The prompt's own message id, so the seat is not quoted back to itself.
    ///
    /// The submitted prompt arrives on the same bus as the reply, as a text
    /// part like any other. Folding it in would post the seat's own prompt to
    /// the room.
    prompt_message: Option<String>,
    /// Part ids in the order they first appeared.
    order: Vec<String>,
    /// The latest state of each part, by id.
    parts: HashMap<String, Value>,
    /// The largest step total the provider reported.
    tokens: u64,
    /// Files the server watched the seat write, first write first.
    files: Vec<String>,
    /// A failure the server reported inside the stream.
    error: Option<String>,
    /// Whether the session has gone busy on this turn's prompt.
    started: bool,
    /// Whether the session has gone idle since.
    finished: bool,
}

impl Watch {
    /// Take one event line from the feed.
    ///
    /// Lines that are not `data:` frames, and frames for another session, are
    /// ignored.
    pub(crate) fn absorb(&mut self, line: &str) {
        let Some(frame) = line.strip_prefix("data:") else {
            return;
        };
        let Ok(event) = serde_json::from_str::<Value>(frame.trim()) else {
            return;
        };
        let properties = event.get("properties").unwrap_or(&Value::Null);
        let session = properties
            .get("sessionID")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(id) = session
            && self.session.is_none()
        {
            self.session = Some(id);
        }
        if !self.mine(properties) {
            return;
        }
        match event.get("type").and_then(Value::as_str) {
            Some("session.status") => {
                if properties.pointer("/status/type").and_then(Value::as_str) == Some("busy") {
                    self.started = true;
                }
            }
            Some("session.idle") => self.finished = true,
            Some("message.updated") => self.note_message(properties),
            Some("message.part.updated") => self.note_part(properties),
            Some("file.edited") => self.note_file(properties),
            Some("session.error") => {
                self.error = Some(describe(properties));
            }
            _ => {}
        }
    }

    /// Whether an event belongs to the session this turn is running in.
    fn mine(&self, properties: &Value) -> bool {
        let Some(mine) = self.session.as_deref() else {
            return true;
        };
        properties
            .get("sessionID")
            .and_then(Value::as_str)
            .is_none_or(|id| id == mine)
    }

    /// Remember the prompt's message id, and any error reported with a reply.
    fn note_message(&mut self, properties: &Value) {
        let info = properties.get("info").unwrap_or(&Value::Null);
        if info.get("role").and_then(Value::as_str) == Some("user")
            && self.prompt_message.is_none()
            && let Some(id) = info.get("id").and_then(Value::as_str)
        {
            self.prompt_message = Some(id.to_string());
        }
        if let Some(error) = info.get("error")
            && !error.is_null()
        {
            self.error = Some(describe(error));
        }
    }

    /// Keep the latest state of one part, in first-seen order.
    fn note_part(&mut self, properties: &Value) {
        let Some(part) = properties.get("part") else {
            return;
        };
        if part.get("messageID").and_then(Value::as_str) == self.prompt_message.as_deref() {
            return;
        }
        if part.get("type").and_then(Value::as_str) == Some("step-finish")
            && let Some(total) = part.pointer("/tokens/total").and_then(Value::as_u64)
        {
            self.tokens = self.tokens.max(total);
        }
        let Some(id) = part.get("id").and_then(Value::as_str) else {
            return;
        };
        if !self.parts.contains_key(id) {
            self.order.push(id.to_string());
        }
        self.parts.insert(id.to_string(), part.clone());
    }

    /// Record a file the seat wrote, without repeats.
    fn note_file(&mut self, properties: &Value) {
        let Some(path) = properties.get("file").and_then(Value::as_str) else {
            return;
        };
        if !self.files.iter().any(|seen| seen == path) {
            self.files.push(path.to_string());
        }
    }

    /// Whether the turn has both started and ended.
    ///
    /// Both halves matter. `finished` alone would be true before the session
    /// ever went busy, which is every moment between the submit and the
    /// provider's first token. A turn that failed before it started is still
    /// over, which is why an error or any part of a reply counts as having
    /// begun.
    pub(crate) fn done(&self) -> bool {
        self.finished && (self.started || !self.order.is_empty() || self.error.is_some())
    }

    /// The session this turn is running in, once one event has named it.
    pub(crate) fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// Fold everything seen into the turn output the desk expects.
    pub(crate) fn finish(&self) -> TurnOutput {
        let mut turn = TurnOutput {
            tokens: self.tokens,
            session: self.session.clone(),
            error: self.error.clone(),
            files_written: self.files.clone(),
            ..TurnOutput::default()
        };
        let mut text = String::new();
        for id in &self.order {
            let Some(part) = self.parts.get(id) else {
                continue;
            };
            match part.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(said) = part.get("text").and_then(Value::as_str) {
                        if !text.is_empty() {
                            text.push('\n');
                        }
                        text.push_str(said);
                    }
                }
                Some("tool") => fold_tool(part, &mut turn),
                _ => {}
            }
        }
        turn.posted = text.contains("<<<POST");
        turn.message = extract_post(&text);
        turn
    }
}

/// Fold one settled tool part into a turn.
fn fold_tool(part: &Value, turn: &mut TurnOutput) {
    let Some(name) = part.get("tool").and_then(Value::as_str) else {
        return;
    };
    turn.tools.push(name.to_string());
    if name == "read" {
        turn.reads += 1;
    }
    let input = part
        .pointer("/state/input")
        .map(Value::to_string)
        .unwrap_or_default();
    let output = part
        .pointer("/state/output")
        .and_then(Value::as_str)
        .unwrap_or_default();
    turn.work_log.push_str("\n$ ");
    turn.work_log.push_str(&crate::agent::truncate(&input, 1200));
    turn.work_log.push('\n');
    turn.work_log.push_str(&crate::agent::truncate(output, 1200));
    turn.work_log.push('\n');
}

/// Pull a readable sentence out of whatever shape an error arrived in.
fn describe(value: &Value) -> String {
    for pointer in ["/error/data/message", "/data/message", "/message", "/name"] {
        if let Some(said) = value.pointer(pointer).and_then(Value::as_str) {
            return said.to_string();
        }
    }
    value.to_string().chars().take(300).collect()
}
