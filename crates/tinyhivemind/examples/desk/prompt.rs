//! Turning a seat's briefing, its history, and this turn's trigger into the
//! one prompt the agent process actually reads.
//!
//! The library says what a seat may see (`TeamBriefing::system_text`,
//! `project_session`) and this host says how that becomes text on the wire,
//! including the standing brief from the desk file and whatever the memory
//! store recalled for this turn.

use std::{collections::BTreeMap, fmt::Write as _};

use tinyhivemind::{SessionAuthor, SessionMessage};

use crate::{
    deskfile,
    notebook::{NOTEBOOK_CHARS, NOTEBOOK_DIR},
    queue::PendingTurn,
};

/// Assemble everything one seat sees for one turn.
///
/// The order matters: who it is, what the desk knows, what the room has said,
/// then what it was actually asked. A model reads the last thing best.
pub(crate) fn compose_prompt(
    briefing: Option<&str>,
    account: Option<&str>,
    history: &[SessionMessage],
    seat: &deskfile::AgentSpec,
    job: &PendingTurn,
    recalled: &str,
    notebook: Option<&str>,
    seats: &[deskfile::AgentSpec],
    spoken: &BTreeMap<String, u64>,
) -> String {
    let mut prompt = match briefing {
        Some(text) => text.to_string(),
        None => format!(
            "You are @{} in this desk. You already hold the briefing and the transcript \
             up to your last turn; what follows is only what has changed since.",
            seat.id
        ),
    };
    prompt.push_str(&who_is_here(seats, &seat.id, spoken));
    prompt.push_str("\n\n## Your standing brief\n");
    prompt.push_str(seat.brief.trim());
    if !recalled.trim().is_empty() {
        prompt.push_str("\n\n## Desk memory (CortexDB)\n");
        prompt.push_str(recalled.trim());
    }
    // The seat's own prior context goes where prior context would have been:
    // after who it is, before what the room said.
    let _ = write!(
        prompt,
        "\n\n## Your notebook (private — `{NOTEBOOK_DIR}/{}.md`, carried from your last turn)\n",
        seat.id
    );
    match notebook {
        Some(text) => prompt.push_str(text),
        None => prompt.push_str("(empty — you have not written one yet; start it this turn)"),
    }
    // Everything older than the live window, as one account the room keeps and
    // rewrites. It is derived and lossy: the messages themselves are still in
    // the transcript at the numbers it cites, and `desk_read` reaches them.
    if let Some(account) = account {
        prompt.push_str("\n\n## The room before that (the desk's standing account)\n");
        prompt.push_str(account);
    }
    prompt.push_str(match briefing {
        Some(_) => "\n\n## The room so far\n",
        None => "\n\n## New in the room since your last turn\n",
    });
    if history.is_empty() {
        prompt.push_str("(nothing new)\n");
    }
    for message in history {
        let who = match &message.author {
            SessionAuthor::Agent { id, .. } => format!("@{id}"),
            SessionAuthor::Person { label, .. } => label.clone(),
            SessionAuthor::Operator => "operator".to_string(),
            SessionAuthor::System { kind, .. } => format!("system/{kind}"),
        };
        let _ = write!(
            prompt,
            "\n[{}] {who}: {}\n",
            message.sequence.0, message.content
        );
    }
    prompt.push_str("\n\n## This turn\n");
    prompt.push_str("You were addressed by this message:\n\n");
    prompt.push_str(job.trigger.trim());
    prompt.push_str("\n\n");
    prompt.push_str(&house_rules(&seat.id));
    prompt
}

/// Who is on this desk right now, and what each of them has done.
///
/// The briefing already lists the seats, but as a static roster: names and
/// roles, with no indication of who is actually carrying the work. A seat
/// deciding who to hand the turn to needs the other half — who has spoken,
/// when, and who has not spoken at all — and deriving that from a bounded
/// window is exactly the thing a window cannot be relied on for. In run 28 the
/// desk spent nine of twelve turns inside one seat while three sat idle, and
/// `desk_dm` went unused across the whole run.
///
/// `spoken` maps a seat id to the sequence number of its most recent message.
fn who_is_here(seats: &[deskfile::AgentSpec], me: &str, spoken: &BTreeMap<String, u64>) -> String {
    let mut text = String::from("\n\n## Who is on this desk right now\n");
    for seat in seats {
        let mine = if seat.id == me { " (you)" } else { "" };
        let state = match spoken.get(&seat.id) {
            Some(at) => format!("last spoke at [{at}]"),
            None => "has not spoken yet".to_string(),
        };
        let _ = write!(text, "- @{}{mine} — {} — {state}\n", seat.id, seat.role);
    }
    text.push_str(
        "\nNaming one of them with @id is what runs them next; naming nobody ends \
         the chain and the chair has to restart the room. Pick the seat whose role \
         fits the open step, not whoever spoke last, and prefer a seat that has not \
         spoken when the work is theirs to do.\n",
    );
    text
}

/// How a seat speaks, what survives its turn, and the rules of the room.
///
/// Split out of [`compose_prompt`] because it is a constant block of text
/// with one seat-dependent path in it, and inlining it pushed the composer
/// past the line limit.
fn house_rules(seat_id: &str) -> String {
    let mut prompt = String::new();
    let _ = write!(
        prompt,
        "Do the work first — use your tools, write and run code in this workspace, check \
         what you claim. Then say ONE thing to the room, by calling a tool.\n\n\
         The room is a tool, not something you write. Text you produce outside a tool \
         call is your own thinking and reaches nobody:\n\
         - `desk_post(message)` — say one thing to the whole desk. Call it once, at the \
           end of your turn. This is how you speak.\n\
         - `desk_dm(to, message)` — say it to named seats instead, when you need one \
           peer to settle something and the room does not need to watch. It still \
           costs your one message for the turn, and the room is told the exchange \
           happened. This is the same mechanism the shared-session rules above call \
           `!aside @peer`; call the tool rather than writing the marker. Use it for \
           a two-seat disagreement, a correction that would otherwise embarrass the \
           room's record, or a question only one seat can answer — not for a result, \
           which belongs to everyone.\n\
         - `desk_read(limit)` — read further back than the window you were handed.\n\
         - `desk_close(message)` — say one last thing AND report the work finished. \
           Use it instead of `desk_post` only when the task is genuinely delivered \
           and no seat has an open step; a result somebody still has to verify is \
           not finished. If you are being asked again about work you already \
           delivered, this is the call that says so.\n\n\
         You are stateless between turns. This process ends when you post, and the \
         next turn starts a fresh one. Four things survive: your notebook, files in \
         this workspace (shared with every seat), what you post to the room, and the \
         desk memory. Before you post:\n\
         - REWRITE `{NOTEBOOK_DIR}/{seat_id}.md` — do not append to it. Write it as the message you \
           want to receive from yourself next turn: what you established, what you \
           are mid-way through, what you would do next, and which files hold what. \
           You will be handed its last {NOTEBOOK_CHARS} characters verbatim. Nobody else reads it.\n\
         - Write working code to named .py files another seat can run, and what the \
           room established to NOTES.md. The room is told which files you wrote; you \
           do not have to list them.\n\
         - Put those files UNDER THIS WORKSPACE, as relative paths. A file you write \
           to /tmp or to any absolute path outside it is not shared and is gone when \
           this process ends, however good the code in it was.\n\n\
         Rules of the room:\n\
         - Exactly one seat speaks per message. Mentioning a teammate with @id runs \
           their turn next, and only the FIRST @mention in your message does that. \
           Everything after it is read as context.\n\
         - So: end with the one seat you actually need, and put it first among \
           your mentions.\n\
         - Never claim a number you did not compute. Say what you ran.\n\
         - Work as long as the problem needs; run as many tools as it takes. But \
           you must finish by calling `desk_post` or `desk_dm`: a turn that never \
           posts is a turn the room never happened, and the work in it reaches \
           nobody.\n\
         - Older messages reach you as the desk's standing account rather than in \
           full. It is written from the messages and can be thin; `desk_read` gets \
           you the messages themselves.\n\
         - If the desk tools are not attached to this session, fall back to wrapping \
           the message in <<<POST and POST>>> and say so in it.\n",
    );
    prompt
}
