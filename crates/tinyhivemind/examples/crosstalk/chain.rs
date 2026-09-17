//! Running the hand-off chain, and the two decisions each turn in it makes:
//! who a committed reply reaches, and whether it opened a private aside.
//!
//! [`run_chain`] is the loop, [`hand_off`] is its call into the
//! mention-dispatch edge, and [`address`] plus the aside bookkeeping below it
//! are how the harness decides an aside's audience without acting on the
//! `!aside` marker itself — that decision belongs to
//! `tinyhivemind_core::aside::aside`.

use crate::agent::{Ask, author_label};
use crate::cli::Options;
use crate::report::Turn;
use crate::room::Room;

use tinyhivemind::dispatch::{
    DispatchConversation, DispatchKey, MentionDispatchInput, MentionDispatchOutcome,
    MentionDispatchPolicy, dispatch_mention,
};
use tinyhivemind::{Conversation, SessionAuthor, SessionQuery, project_session};
use tinyhivemind_core::aside::{AsideDecision, AsideInput, AsidePolicy, Audience, Viewer, aside};
use tinyhivemind_core::mention::{
    Mention, MentionAuthor, MentionTarget, resolve as resolve_mentions,
};

/// The aside policy the harness runs under when `--aside` is given.
///
/// A pair, four rows, and a settlement owed before another may be opened —
/// small on purpose, because the point is to watch the mechanism rather than
/// to give a room somewhere to hide.
const ASIDES: AsidePolicy = AsidePolicy {
    enabled: true,
    max_members: 1,
    max_messages: 4,
    must_surface: true,
    require_thread: false,
};

/// Run the hand-off chain until the library stops it.
///
/// One turn per iteration, and at most one child turn per turn — that bound is
/// the library's, not this loop's: `mention_dispatch` returns a decision that
/// can carry exactly one request, and there is no variant that carries two.
/// The loop ends when a turn addresses nobody, addresses itself, exhausts the
/// hop budget, or the host's queue refuses it, and the reason is recorded on
/// the last turn rather than inferred afterwards.
pub(crate) async fn run_chain(
    options: &Options,
    room: &Room<'_>,
    floor: &Conversation,
    first: &str,
) -> Result<(Vec<Turn>, Vec<String>), String> {
    let Room {
        seats,
        ids,
        journal,
        queue,
        roster,
        desks,
    } = room;
    let mut turns: Vec<Turn> = Vec::new();
    let mut refusals: Vec<String> = Vec::new();
    let mut speaker = first.to_owned();
    let mut hop = 0_u32;
    let mut carried: Option<(String, String)> = None;

    loop {
        let seat = seats
            .iter()
            .find(|seat| seat.id == speaker)
            .ok_or_else(|| format!("no seat for {speaker}"))?;

        let visible = project_session(
            journal.as_ref(),
            &SessionQuery {
                conversation: floor.clone(),
                before: None,
                window: options.window,
                viewer: Viewer::Agent {
                    id: seat.id.clone(),
                },
            },
        )
        .await
        .map_err(|error| format!("projection failed: {error}"))?;

        let ask = match &carried {
            Some((from, content)) => Ask::Addressed { from, content },
            None => Ask::Desk,
        };
        let peers: Vec<&str> = ids
            .iter()
            .filter(|id| **id != seat.id.as_str())
            .copied()
            .collect();
        let line = seat.speak(&visible, &peers, &ask, options.asides)?;

        let mentions = resolve_mentions(
            &line,
            None,
            &MentionAuthor::Agent {
                id: seat.id.clone(),
            },
            roster,
            desks,
        );

        let audience = address(
            options,
            floor,
            roster,
            desks,
            &seat.id,
            &line,
            &mentions,
            &turns,
            &mut refusals,
        )?;

        let sequence = journal.append_to(
            floor,
            SessionAuthor::Agent {
                id: seat.id.clone(),
                label: seat.id.clone(),
            },
            &line,
            audience.clone(),
        );
        let outcome = hand_off(
            options, queue, roster, floor, &seat.id, &line, mentions, sequence, hop,
        )
        .await?;

        turns.push(Turn {
            sequence,
            audience,
            speaker: seat.id.clone(),
            hop,
            thread_root: floor.thread_root,
            content: line,
            saw: visible
                .iter()
                .map(|message| format!("{}:{}", message.sequence, author_label(&message.author)))
                .collect(),
            outcome: outcome.clone(),
        });

        if !matches!(outcome, MentionDispatchOutcome::Enqueued) {
            break;
        }
        let Some(enqueued) = queue.drain().into_iter().next() else {
            break;
        };
        speaker.clone_from(&enqueued.request.target_id);
        hop = enqueued.request.child_hop;
        carried = Some((enqueued.request.source_id, enqueued.request.content));
    }
    Ok((turns, refusals))
}

/// Offer this committed reply to the mention-dispatch edge.
///
/// One call, one decision, at most one child turn. The policy is the host's
/// and is passed explicitly on every call: the library adds no default and no
/// smaller ceiling of its own.
#[allow(clippy::too_many_arguments)]
async fn hand_off(
    options: &Options,
    queue: &crate::host::Queue,
    roster: &tinyhivemind_core::roster::Roster<'_>,
    floor: &Conversation,
    author_id: &str,
    line: &str,
    mentions: Vec<Mention>,
    sequence: tinyhivemind::Sequence,
    hop: u32,
) -> Result<MentionDispatchOutcome, String> {
    dispatch_mention(
        queue,
        MentionDispatchPolicy {
            enabled: true,
            max_hops: options.hops,
        },
        &MentionDispatchInput {
            key: DispatchKey {
                trigger_sequence: sequence.0,
            },
            conversation: DispatchConversation::from(floor),
            author_id: author_id.to_owned(),
            content: line.to_owned(),
            mentions,
            hop,
        },
        roster,
    )
    .await
    .map_err(|error| format!("dispatch failed: {error}"))
}

/// Decide who one authored line is addressed to.
///
/// A line asking for an aside is addressed by the library, not by this harness
/// reading the marker and believing it: `aside` resolves who it may reach and
/// refuses with a named reason otherwise. A refusal leaves the row
/// desk-visible, which is the safe direction to fail in.
#[allow(clippy::too_many_arguments)]
fn address(
    options: &Options,
    floor: &Conversation,
    roster: &tinyhivemind_core::roster::Roster<'_>,
    desks: &tinyhivemind_core::desk::DeskSet<'_>,
    author_id: &str,
    line: &str,
    mentions: &[Mention],
    turns: &[Turn],
    refusals: &mut Vec<String>,
) -> Result<Audience, String> {
    if !options.asides || !line.trim_start().starts_with("!aside") {
        return Ok(Audience::Desk);
    }
    let decision = aside(
        ASIDES,
        &AsideInput {
            conversation: DispatchConversation::from(floor),
            author_id: author_id.to_owned(),
            mentions: mentions.to_vec(),
            spent: spent_in_aside(turns),
            unsettled: unsettled_aside(turns, author_id, mentions),
        },
        roster,
        desks,
    )
    .map_err(|error| format!("the aside fold failed: {error}"))?;
    Ok(match decision {
        AsideDecision::One { audience } => audience,
        AsideDecision::None { reason } => {
            refusals.push(format!("{reason:?}"));
            Audience::Desk
        }
    })
}

/// How many rows the currently open aside has already spent.
///
/// Folded from the turns the run has taken rather than stored, because this
/// harness holds no state the journal does not already carry. Every row in
/// the run of consecutive non-desk turns counts, not only the ones this
/// speaker wrote: two peers alternating inside the same aside share one
/// budget.
fn spent_in_aside(turns: &[Turn]) -> usize {
    turns
        .iter()
        .rev()
        .take_while(|turn| !turn.audience.is_desk())
        .count()
}

/// Whether a *different* prior aside among the same participants has not yet
/// surfaced.
///
/// `mentions` is the current line's addressed targets, resolved the same way
/// [`aside`] resolves them: quiet mentions and the author itself are dropped
/// before the set is compared. Replying inside the aside already in progress
/// never counts, no matter how unsettled an earlier one was — otherwise the
/// very message the policy is meant to allow would refuse itself, and every
/// demonstrated aside would top out at one row. Walking forward through the
/// turns already taken, an aside opens the run when its own participant
/// set — author plus addressed members — matches this line's, and closes it
/// the first time one of those participants speaks on the desk afterwards.
/// What is left open at the end, once the current one is excluded, is what
/// `aside` must refuse to add to.
fn unsettled_aside(turns: &[Turn], author_id: &str, mentions: &[Mention]) -> bool {
    let mut participants: Vec<&str> = mentions
        .iter()
        .filter(|mention| !mention.quiet)
        .filter_map(|mention| match &mention.target {
            MentionTarget::Agent { id } if id != author_id => Some(id.as_str()),
            _ => None,
        })
        .collect();
    if participants.is_empty() {
        return false;
    }
    participants.push(author_id);
    participants.sort_unstable();
    participants.dedup();

    // Replying inside the aside already in progress is not opening another
    // one — `spent_in_aside`'s own budget is what ends that run, at
    // `max_messages`. Only starting a *different* aside while an earlier one
    // among these exact participants has not surfaced should be refused, so a
    // line whose immediately preceding turn already carries this same
    // participant set is a continuation, not a candidate to check here.
    if let Some(Turn {
        audience: Audience::Aside { members },
        speaker,
        ..
    }) = turns.last()
    {
        let mut current: Vec<&str> = members
            .iter()
            .map(String::as_str)
            .chain(std::iter::once(speaker.as_str()))
            .collect();
        current.sort_unstable();
        current.dedup();
        if current == participants {
            return false;
        }
    }

    let mut open = false;
    for turn in turns {
        match &turn.audience {
            Audience::Aside { members } => {
                let mut turn_participants: Vec<&str> = members
                    .iter()
                    .map(String::as_str)
                    .chain(std::iter::once(turn.speaker.as_str()))
                    .collect();
                turn_participants.sort_unstable();
                turn_participants.dedup();
                if turn_participants == participants {
                    open = true;
                }
            }
            Audience::Desk => {
                if open && participants.contains(&turn.speaker.as_str()) {
                    open = false;
                }
            }
        }
    }
    open
}
