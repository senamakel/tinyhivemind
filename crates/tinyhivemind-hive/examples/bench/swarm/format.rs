//! The wire text a swarm's members exchange, and the trace line a run keeps.
//!
//! A referral carries a plain line of text between desks, exactly as a real
//! agent's reply would. [`readings`] and [`restate`] are the two halves of
//! that convention: `readings` reads a desk's numeric view of every option out
//! of a line written the way a member would write it, and `restate` writes a
//! set of readings back out in the same form, so a reply that carries them on
//! can be read the same way at the far end. [`line`] is unrelated to the wire
//! format itself — it renders one transcript entry for the `--trace` view.

use std::fmt::Write as _;

use tinyhivemind_hive::{Sequence, trace::TopicId};

use super::Channel;

/// One desk's reading of one option, as it appears in a message.
#[derive(Clone, Debug)]
pub(super) struct Reading {
    /// The desk that holds the reading, by display name.
    pub(super) desk: String,
    /// The option it is a reading of.
    pub(super) topic: TopicId,
    /// What that desk rates it.
    pub(super) value: i32,
}

/// Write a set of readings back out in the form they were read from.
pub(super) fn restate(readings: &[Reading]) -> String {
    let mut line = String::new();
    let mut current: Option<&str> = None;
    for reading in readings {
        if current.is_none_or(|desk| desk != reading.desk) {
            if current.is_some() {
                line.push_str(". ");
            }
            let _ = write!(line, "{} reads", reading.desk);
            current = Some(reading.desk.as_str());
        } else {
            line.push(',');
        }
        let _ = write!(line, " #{} at {}", reading.topic, reading.value);
    }
    if current.is_some() {
        line.push('.');
    }
    line
}

/// Read every `<Desk> reads #option at <n>, #option at <n>.` clause out of a
/// message.
///
/// The form is the one the members write, and it is meant to survive a real
/// agent writing it in a sentence of its own rather than as a field: a desk
/// name followed by `reads` opens a clause, and every `#option at <n>` until
/// the next desk name belongs to it.
pub(super) fn readings(content: &str) -> Vec<Reading> {
    let words: Vec<&str> = content.split_whitespace().collect();
    let mut found = Vec::new();
    let mut desk: Option<String> = None;
    for (at, word) in words.iter().enumerate() {
        if *word == "reads"
            && let Some(name) = at.checked_sub(1).and_then(|before| words.get(before))
        {
            desk = Some((*name).to_owned());
            continue;
        }
        let Some(topic) = word.strip_prefix('#') else {
            continue;
        };
        let topic = topic.trim_end_matches(['.', ',', ';', '?', '!']);
        if topic.is_empty() || words.get(at + 1) != Some(&"at") {
            continue;
        }
        let Some(value) = words.get(at + 2) else {
            continue;
        };
        let Ok(value) = value.trim_end_matches(['.', ',', ';']).parse::<i32>() else {
            continue;
        };
        let Some(desk) = desk.clone() else {
            continue;
        };
        found.push(Reading {
            desk,
            topic: TopicId::from(topic),
            value,
        });
    }
    found
}

/// One transcript line, tagged with the channel it was written in.
pub(super) fn line(
    channels: &[Channel],
    desk: usize,
    sequence: Sequence,
    agent_id: &str,
    content: &str,
) -> String {
    format!(
        "{:>9} {:>3}  {:<18} {content}",
        channels[desk].name, sequence.0, agent_id,
    )
}
