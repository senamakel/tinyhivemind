//! Tests holding every [`ResponderRung`] and [`SelectionDisposition`] to the
//! disclosure classification fixed by
//! `docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::super::*;

/// What `docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md`
/// requires of one outcome's rendered sentence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Rendering {
    /// The outcome turns on a named other, so it shares one sentence with
    /// every other outcome that does.
    Shared,
    /// The outcome is about the caller's own request, so it renders its own.
    Own,
}

/// Every rung, so a rendering can be asserted over the whole enum.
const EVERY_RUNG: [ResponderRung; 5] = [
    ResponderRung::ExplicitMention,
    ResponderRung::AutoSelection,
    ResponderRung::DeskDefault,
    ResponderRung::DirectAgent,
    ResponderRung::Orchestrator,
];

/// Every disposition, so a rendering can be asserted over the whole enum.
const EVERY_DISPOSITION: [SelectionDisposition; 5] = [
    SelectionDisposition::NotApplicable,
    SelectionDisposition::Selected,
    SelectionDisposition::Disabled,
    SelectionDisposition::Unavailable,
    SelectionDisposition::InvalidOutput,
];

/// The classification ADR 0009 fixes for each rung.
///
/// The ladder withholds nothing: every decision names a responder, and the
/// rung only says which step produced the id the caller is already reading.
/// Nothing here can be probed for a fact about anybody else, so every rung
/// renders its own sentence.
///
/// The match is wildcard-free on purpose: a rung added later does not compile
/// until whoever adds it has classified it here.
fn required_of_rung(rung: ResponderRung) -> Rendering {
    match rung {
        ResponderRung::ExplicitMention
        | ResponderRung::AutoSelection
        | ResponderRung::DeskDefault
        | ResponderRung::DirectAgent
        | ResponderRung::Orchestrator => Rendering::Own,
    }
}

/// The classification ADR 0009 fixes for each disposition.
///
/// `Unavailable` and `InvalidOutput` stay separable. Both are facts about the
/// host's own selector and about a fallback that has already run; neither
/// turns on whether some named agent exists, and both arrive beside the
/// responder id they explain.
fn required_of_disposition(disposition: SelectionDisposition) -> Rendering {
    match disposition {
        SelectionDisposition::NotApplicable
        | SelectionDisposition::Selected
        | SelectionDisposition::Disabled
        | SelectionDisposition::Unavailable
        | SelectionDisposition::InvalidOutput => Rendering::Own,
    }
}

#[test]
fn every_rung_renders_its_own_sentence() {
    let mut seen: Vec<String> = Vec::new();
    for rung in EVERY_RUNG {
        let sentence = rung.to_string();
        match required_of_rung(rung) {
            Rendering::Own => assert!(!seen.contains(&sentence), "{rung:?} repeats a sentence"),
            // A rung that turned on a named other would have to be worded
            // exactly like every other such rung, which is the assertion in
            // `every_withheld_outcome_shares_one_sentence`.
            Rendering::Shared => (),
        }
        seen.push(sentence);
    }
}

#[test]
fn every_disposition_renders_its_own_sentence() {
    let mut seen: Vec<String> = Vec::new();
    for disposition in EVERY_DISPOSITION {
        let sentence = disposition.to_string();
        if required_of_disposition(disposition) == Rendering::Own {
            assert!(
                !seen.contains(&sentence),
                "{disposition:?} repeats a sentence"
            );
        }
        seen.push(sentence);
    }
}

#[test]
fn every_withheld_outcome_shares_one_sentence() {
    let withheld: Vec<String> = EVERY_RUNG
        .into_iter()
        .filter(|rung| required_of_rung(*rung) == Rendering::Shared)
        .map(|rung| rung.to_string())
        .chain(
            EVERY_DISPOSITION
                .into_iter()
                .filter(|disposition| required_of_disposition(*disposition) == Rendering::Shared)
                .map(|disposition| disposition.to_string()),
        )
        .collect();
    // Empty today, and that is the finding: the ladder always names a
    // responder, so it has nothing to withhold. A variant classified `Shared`
    // later must be worded identically to every other one.
    for sentence in &withheld {
        assert_eq!(sentence, &withheld[0]);
    }
}

#[test]
fn every_outcome_is_a_lowercase_sentence_without_trailing_punctuation() {
    let sentences = EVERY_RUNG
        .into_iter()
        .map(|rung| (format!("{rung:?}"), rung.to_string()))
        .chain(
            EVERY_DISPOSITION
                .into_iter()
                .map(|disposition| (format!("{disposition:?}"), disposition.to_string())),
        );
    for (name, sentence) in sentences {
        assert!(!sentence.is_empty(), "{name}");
        assert_eq!(sentence, sentence.to_lowercase(), "{name}");
        assert!(!sentence.ends_with(['.', '!', '?']), "{name}");
    }
}

#[test]
fn renders_the_settled_sentences() {
    assert_eq!(
        ResponderRung::ExplicitMention.to_string(),
        "the agent this message named answered it"
    );
    assert_eq!(
        ResponderRung::AutoSelection.to_string(),
        "this desk picked whoever suited the message best"
    );
    assert_eq!(
        ResponderRung::DeskDefault.to_string(),
        "nobody in particular was named, so this desk's first agent answered"
    );
    assert_eq!(
        ResponderRung::DirectAgent.to_string(),
        "this conversation has one agent, and it answered"
    );
    assert_eq!(
        ResponderRung::Orchestrator.to_string(),
        "no desk agent applied here, so the coordinator answered"
    );
    assert_eq!(
        SelectionDisposition::NotApplicable.to_string(),
        "no model was asked to choose who answers"
    );
    assert_eq!(
        SelectionDisposition::Selected.to_string(),
        "a model chose who answers"
    );
    assert_eq!(
        SelectionDisposition::Disabled.to_string(),
        "choosing who answers by model is turned off here"
    );
    assert_eq!(
        SelectionDisposition::Unavailable.to_string(),
        "no model was available to choose, so this desk's first agent answered"
    );
    assert_eq!(
        SelectionDisposition::InvalidOutput.to_string(),
        "the model did not name one agent, so this desk's first agent answered"
    );
}
