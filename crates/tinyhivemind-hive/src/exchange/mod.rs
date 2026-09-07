//! Private exchange that does not take the floor.
//!
//! An aside that rides alongside a turn is free but rationed: only the member
//! the attention market authorized may write one, so a room converging in
//! eleven turns writes at most eleven private rows. This module lifts that
//! ration. Between turns, a host may run an **exchange round** in which each
//! eligible member appends at most one private row, taking no floor and
//! producing no turn.
//!
//! The whole safety argument is one property, and it belongs to the algebra
//! rather than to this module: a private row is dropped by the episode's own
//! fold before it can reach a trace, a standing, the sequence they fold at,
//! the directory or the floor, and `EpisodeState::spent` counts turns rather
//! than rows. [`crate::step`] therefore returns the same step — including the
//! state it commits — whether an exchange happened or not, which
//! `tests/fuzz_invariants.rs` asserts over arbitrary transcripts.
//!
//! What a round spends is model calls, not turns. That is a real cost and a
//! host's to authorize, so [`ExchangePolicy`] is off by default and carries
//! two finite ceilings a host can read its worst case off before starting.
//!
//! See [`docs/specs/off-floor-exchange.md`][spec] and [ADR 0012][adr].
//!
//! [spec]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/specs/off-floor-exchange.md
//! [adr]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0012-an-exchange-round-spends-model-calls-not-turns.md

#[cfg(test)]
mod test;

mod types;

pub use types::{ExchangePolicy, ExchangeRound, NoExchangeReason};

use tinyhivemind::{SessionAuthor, SessionMessage, desk::DeskSet, roster::Roster};

use crate::{EpisodeState, Result};

/// Whether members may exchange privately now, and which of them.
///
/// A fold over what the caller already holds: the policy, the episode's state,
/// the transcript, and the two snapshots. Nothing is stored between calls —
/// how much a member has spent is *read back* out of the private rows above
/// the watermark that it authored, so there is no counter that could disagree
/// with the log.
///
/// The round names members; it does not name pairs. Who a member wants to
/// contact is its own judgement, and the audience it writes is validated by
/// [`tinyhivemind::aside::aside`] exactly as an aside riding alongside a turn is.
///
/// # Errors
///
/// Returns [`crate::Error::Core`] when the roster or desk snapshot is
/// structurally invalid, or when the episode's desk is unknown.
pub fn exchange(
    policy: &ExchangePolicy,
    state: &EpisodeState,
    transcript: &[SessionMessage],
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
) -> Result<ExchangeRound> {
    roster.validate()?;
    desks.validate()?;
    let desk_id = desks.resolve_id(&state.conversation.desk_id)?;
    let members: Vec<&str> = desks
        .members(desk_id)?
        .into_iter()
        .filter(|id| roster.active_member(id).is_some())
        .collect();

    let closed = |reason| Ok(ExchangeRound::Closed { reason });
    if !policy.enabled || policy.contact_cap == 0 || policy.round_cap == 0 {
        return closed(NoExchangeReason::Disabled);
    }
    // A pair is the smallest thing that can exchange. One member alone has
    // nobody to contact, and a round that named it would authorize a row that
    // no audience could admit.
    if members.len() < 2 {
        return closed(NoExchangeReason::TooFewMembers);
    }

    let spent = spent_by(transcript, state, &members);
    // Rounds are counted by the busiest member: a round authorizes each member
    // at most one row, so nobody can have written more rows than there have
    // been rounds. Reading it this way keeps the count folded rather than
    // carried, at the cost of under-counting a round in which every named
    // member declined to write. That is *not* the safe direction: it lets
    // `RoundsSpent` arrive later than the true round count, so a host whose
    // participants decline every round can call `exchange` — and pay for
    // asking each named member — more than `round_cap` times.
    //
    // A correct count needs a marker for "this round opened" independent of
    // any row it produced, and this fold has nothing to read one from: no
    // row is dropped by a decline, so no artifact of a declined round exists
    // in the transcript, and this crate stores no counter of its own (see the
    // module doc). Closing that gap means the host recording round-opened,
    // not just rows-written — a protocol change, tracked as an open question
    // in `docs/specs/off-floor-exchange.md` rather than solved by this fold.
    // In the meantime the practical ceiling on model calls is
    // `min(round_cap, turns already taken + 1) × members`, since a host only
    // opens a round between turns.
    let rounds = spent.iter().map(|(_, count)| *count).max().unwrap_or(0);
    if rounds >= policy.round_cap {
        return closed(NoExchangeReason::RoundsSpent);
    }

    let eligible: Vec<String> = members
        .iter()
        .filter(|id| contacts_of(&spent, id) < policy.contact_cap)
        .map(|id| (*id).to_owned())
        .collect();
    if eligible.is_empty() {
        return closed(NoExchangeReason::ContactsSpent);
    }

    // What the episode may still write in total, so a host can size a round
    // without recomputing the fold. Saturating because a host that exceeded
    // its authorization should read zero rather than wrap.
    //
    // Clamped by the rounds still open: each remaining round authorizes at
    // most one row per eligible member, so a generous `contact_cap` must not
    // be reported as reachable when `round_cap` would close the episode
    // first. Without this clamp a host sizing its remaining budget off this
    // field alone would overallocate.
    let by_contact_cap = members
        .iter()
        .map(|id| policy.contact_cap.saturating_sub(contacts_of(&spent, id)))
        .fold(0_u32, u32::saturating_add);
    let rounds_left = policy.round_cap.saturating_sub(rounds);
    let by_round_cap = rounds_left.saturating_mul(u32::try_from(members.len()).unwrap_or(u32::MAX));
    let remaining = by_contact_cap.min(by_round_cap);

    Ok(ExchangeRound::Open {
        members: eligible,
        remaining,
    })
}

/// Private rows above the watermark, counted by the member that authored them.
///
/// Only rows from current members of this desk count. A row from a retired
/// agent, or from one that belongs to another desk, is not this episode's
/// spend — the same test `live_traces` applies before folding a vote.
fn spent_by<'a>(
    transcript: &[SessionMessage],
    state: &EpisodeState,
    members: &[&'a str],
) -> Vec<(&'a str, u32)> {
    let mut spent: Vec<(&str, u32)> = members.iter().map(|id| (*id, 0)).collect();
    for message in transcript {
        if message.sequence <= state.watermark || message.audience.is_desk() {
            continue;
        }
        let SessionAuthor::Agent { id, .. } = &message.author else {
            continue;
        };
        if let Some(entry) = spent.iter_mut().find(|(member, _)| member == id) {
            entry.1 = entry.1.saturating_add(1);
        }
    }
    spent
}

/// One member's spend, or zero for a member the fold did not see.
fn contacts_of(spent: &[(&str, u32)], id: &str) -> u32 {
    spent
        .iter()
        .find(|(member, _)| *member == id)
        .map_or(0, |(_, count)| *count)
}
