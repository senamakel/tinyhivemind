//! Parsing for the plain-text desk file that describes one live room.
//!
//! A desk file names the desk, the person who chairs it, and one block per
//! agent. Everything an agent is told about itself beyond the library's own
//! briefing lives in its `brief:` lines, so changing the room is editing text
//! rather than editing code.

use std::{collections::BTreeSet, fmt};

/// One agent seat in a desk file.
#[derive(Clone, Debug)]
pub struct AgentSpec {
    /// Canonical mention id, lowercase and stable.
    pub id: String,
    /// Display label shown in the transcript.
    pub label: String,
    /// One-line role, handed to the responder ladder's selector.
    pub role: String,
    /// The private brief prepended to every turn this seat takes.
    pub brief: String,
}

/// A parsed desk file.
#[derive(Clone, Debug)]
pub struct DeskSpec {
    /// Canonical desk id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// The person who opens the room and nudges it.
    pub person_id: String,
    /// The person's display label.
    pub person_label: String,
    /// The seats, in the order the file declares them.
    pub agents: Vec<AgentSpec>,
}

/// Why a desk file could not be read.
#[derive(Debug)]
pub struct ParseError(String);

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "desk file: {}", self.0)
    }
}

impl std::error::Error for ParseError {}

impl DeskSpec {
    /// Look up a seat by its canonical id.
    pub fn agent(&self, id: &str) -> Option<&AgentSpec> {
        self.agents.iter().find(|agent| agent.id == id)
    }
}

/// Parse a desk file.
///
/// # Errors
///
/// Returns [`ParseError`] when a required header is missing, an agent block is
/// malformed, or two seats share an id.
pub fn parse(text: &str) -> Result<DeskSpec, ParseError> {
    let mut id = None;
    let mut name = None;
    let mut person_id = None;
    let mut person_label = None;
    let mut agents: Vec<AgentSpec> = Vec::new();
    let mut current: Option<AgentSpec> = None;

    for raw in text.lines() {
        let line = raw.trim_end();
        if line.trim_start().starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("[agent ").and_then(|r| r.strip_suffix(']')) {
            if let Some(agent) = current.take() {
                agents.push(agent);
            }
            let seat = rest.trim().to_string();
            if seat.is_empty() {
                return Err(ParseError("an [agent ...] block has no id".into()));
            }
            current = Some(AgentSpec {
                label: seat.clone(),
                id: seat,
                role: String::new(),
                brief: String::new(),
            });
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            // A continuation line inside a brief.
            if let Some(agent) = current.as_mut()
                && !line.trim().is_empty()
            {
                agent.brief.push('\n');
                agent.brief.push_str(line.trim());
            }
            continue;
        };
        let key = key.trim();
        let value = value.trim().to_string();
        match (current.as_mut(), key) {
            (None, "desk") => id = Some(value),
            (None, "name") => name = Some(value),
            (None, "person") => person_id = Some(value),
            (None, "person_label") => person_label = Some(value),
            (Some(agent), "name") => agent.label = value,
            (Some(agent), "role") => agent.role = value,
            (Some(agent), "brief") => {
                if !agent.brief.is_empty() {
                    agent.brief.push('\n');
                }
                agent.brief.push_str(&value);
            }
            _ => {
                if let Some(agent) = current.as_mut() {
                    agent.brief.push('\n');
                    agent.brief.push_str(line.trim());
                }
            }
        }
    }
    if let Some(agent) = current.take() {
        agents.push(agent);
    }

    let id = id.ok_or_else(|| ParseError("missing `desk:`".into()))?;
    let name = name.unwrap_or_else(|| id.clone());
    let person_id = person_id.ok_or_else(|| ParseError("missing `person:`".into()))?;
    let person_label = person_label.unwrap_or_else(|| person_id.clone());
    if agents.is_empty() {
        return Err(ParseError("no [agent ...] blocks".into()));
    }
    let mut seen = BTreeSet::new();
    for agent in &agents {
        if !seen.insert(agent.id.clone()) {
            return Err(ParseError(format!("duplicate seat `{}`", agent.id)));
        }
        if agent.id == person_id {
            return Err(ParseError(format!(
                "seat `{}` collides with the chair",
                agent.id
            )));
        }
    }
    Ok(DeskSpec {
        id,
        name,
        person_id,
        person_label,
        agents,
    })
}
