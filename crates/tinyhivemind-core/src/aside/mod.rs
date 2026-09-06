//! Pure authorization of one bounded private aside inside a desk.
//!
//! An aside is a run of rows addressed to fewer readers than the conversation
//! holds. This module decides whether one may be opened and resolves exactly
//! who it is addressed to; it stores nothing, and the row it authorizes is
//! appended through the same path every other message uses.
//!
//! Two rules give the mechanism its shape, and both are enforced elsewhere
//! rather than here, because neither is a decision this fold could make:
//!
//! - **A redaction is a row, not an absence.** A reader outside the audience
//!   still sees that the exchange happened, who wrote it and to whom. That is
//!   what makes an aside auditable rather than a covert channel, and it is
//!   what lets a peer know there is something to ask about.
//! - **An aside carries information, never support.** A deliberation trace in
//!   a row whose audience is not [`Audience::Desk`] contributes to no standing,
//!   identically for every reader. To make an aside count, a member spends a
//!   desk-visible turn saying so in the open.
//!
//! See [`docs/specs/private-asides.md`][spec] and [ADR 0008][adr].
//!
//! [spec]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/specs/private-asides.md
//! [adr]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0008-an-aside-carries-information-never-support.md

#[cfg(test)]
mod test;

mod types;

pub use types::{Audience, AsideDecision, AsideInput, AsidePolicy, NoAsideReason, Viewer};

use crate::{
    chat::is_general_chat,
    desk::DeskSet,
    error::Result,
    mention::MentionTarget,
    roster::Roster,
};

/// Decide whether one authored message may address fewer readers than its desk.
///
/// Evaluation is fail-closed and ordered: each rung is checked before the next,
/// and the first addressed id that cannot be resolved stops the decision rather
/// than being skipped in favour of a later one. That matches
/// [`mention_dispatch`](crate::dispatch::mention_dispatch) — an audience
/// assembled by quietly dropping the names it could not resolve is not the
/// audience the author wrote.
///
/// # Errors
///
/// Returns a typed core error when the supplied roster or desk snapshot is
/// malformed.
pub fn aside(
    policy: AsidePolicy,
    input: &AsideInput,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
) -> Result<AsideDecision> {
    let none = |reason| AsideDecision::None { reason };
    if !policy.enabled {
        return Ok(none(NoAsideReason::Disabled));
    }
    roster.validate()?;
    desks.validate()?;

    if roster.active_member(&input.author_id).is_none() {
        return Ok(none(NoAsideReason::SourceInactive));
    }

    let desk = &input.conversation.desk_id;
    if !on_desk(&input.author_id, desk, desks) {
        return Ok(none(NoAsideReason::AuthorNotOnDesk));
    }

    if policy.require_thread && input.conversation.thread_root.is_none() {
        return Ok(none(NoAsideReason::ThreadRequired));
    }
    if input.spent >= policy.max_messages {
        return Ok(none(NoAsideReason::BudgetSpent));
    }
    if policy.must_surface && input.unsettled {
        return Ok(none(NoAsideReason::UnsettledAside));
    }

    // Addressed ids in reading order, deduplicated on first appearance, with
    // the author removed: an author is admitted to its own row by definition,
    // so naming itself neither widens nor narrows the audience.
    let mut ordered: Vec<_> = input.mentions.iter().collect();
    ordered.sort_by_key(|mention| mention.offset);

    let mut named = false;
    let mut members: Vec<String> = Vec::new();
    for mention in ordered {
        if mention.quiet {
            continue;
        }
        let MentionTarget::Agent { id } = &mention.target else {
            continue;
        };
        named = true;
        if *id == input.author_id {
            continue;
        }
        if members.iter().any(|member| member == id) {
            continue;
        }
        if roster.active_member(id).is_none() {
            return Ok(none(NoAsideReason::TargetInactive));
        }
        if !on_desk(id, desk, desks) {
            return Ok(none(NoAsideReason::TargetNotOnDesk));
        }
        members.push(id.clone());
    }

    if members.is_empty() {
        return Ok(none(if named {
            NoAsideReason::SelfOnly
        } else {
            NoAsideReason::NoAudience
        }));
    }
    if members.len() > policy.max_members {
        return Ok(none(NoAsideReason::AudienceTooLarge));
    }

    Ok(AsideDecision::One {
        audience: Audience::Aside { members },
    })
}

/// Whether `agent_id` is an effective member of the desk this row is on.
///
/// General has no membership list — every active agent is present — so the
/// desk check there is roster activity and nothing more. That is the same
/// reading `referral` takes of a deskless conversation, and taking a different
/// one here would make an aside possible in General for nobody.
fn on_desk(agent_id: &str, desk_id: &str, desks: &DeskSet<'_>) -> bool {
    if is_general_chat(Some(desk_id)) {
        return true;
    }
    desks
        .members(desk_id)
        .is_ok_and(|members| members.iter().any(|member| *member == agent_id))
}
