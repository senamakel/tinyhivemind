//! Unit tests for what one seat is told before it takes a turn.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;

fn seats() -> Vec<deskfile::AgentSpec> {
    ["theory", "solver", "checker"]
        .into_iter()
        .map(|id| deskfile::AgentSpec {
            id: id.into(),
            label: id.into(),
            role: format!("{id} role"),
            brief: format!("{id} brief"),
        })
        .collect()
}

#[test]
fn names_every_seat_and_marks_the_one_reading_it() {
    let spoken = BTreeMap::from([("theory".to_string(), 7_u64)]);
    let text = who_is_here(&seats(), "solver", &spoken);
    assert!(text.contains("@theory — theory role — last spoke at [7]"));
    assert!(
        text.contains("@solver (you)"),
        "the reader is marked: {text}"
    );
    assert!(
        text.contains("@checker — checker role — has not spoken yet"),
        "a silent seat is named as silent, which is the point: {text}"
    );
}

#[test]
fn says_that_naming_a_seat_is_what_runs_it() {
    let text = who_is_here(&seats(), "theory", &BTreeMap::new());
    assert!(
        text.contains("is what runs them next"),
        "a seat that does not know this ends the chain: {text}"
    );
}

#[test]
fn a_desk_where_nobody_has_spoken_still_lists_everybody() {
    let text = who_is_here(&seats(), "lead", &BTreeMap::new());
    for id in ["@theory", "@solver", "@checker"] {
        assert!(text.contains(id), "{id} missing from {text}");
    }
    assert!(!text.contains("(you)"), "no seat here is the reader");
}
