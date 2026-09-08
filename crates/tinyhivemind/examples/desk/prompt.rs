//! Turning a seat's briefing, its history, and this turn's trigger into the
//! one prompt the agent process actually reads.
//!
//! The library says what a seat may see (`TeamBriefing::system_text`,
//! `project_session`) and this host says how that becomes text on the wire,
//! including the standing brief from the desk file and whatever the memory
//! store recalled for this turn.

use std::fmt::Write as _;

use tinyhivemind::{SessionAuthor, SessionMessage};

use crate::{deskfile, queue::PendingTurn};

/// Assemble everything one seat sees for one turn.
///
/// The order matters: who it is, what the desk knows, what the room has said,
/// then what it was actually asked. A model reads the last thing best.
pub(crate) fn compose_prompt(
    briefing: Option<&str>,
    history: &[SessionMessage],
    seat: &deskfile::AgentSpec,
    job: &PendingTurn,
    recalled: &str,
) -> String {
    let mut prompt = match briefing {
        Some(text) => text.to_string(),
        None => format!(
            "You are @{} in this desk. You already hold the briefing and the transcript \
             up to your last turn; what follows is only what has changed since.",
            seat.id
        ),
    };
    prompt.push_str("\n\n## Your standing brief\n");
    prompt.push_str(seat.brief.trim());
    if !recalled.trim().is_empty() {
        prompt.push_str("\n\n## Desk memory (CortexDB)\n");
        prompt.push_str(recalled.trim());
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
    prompt.push_str(
        "Do the work first — use your tools, write and run code in this workspace, check \
         what you claim. Then post ONE message to the room.\n\n\
         You are stateless between turns. This process ends when you post, and the \
         next turn starts a fresh one. Only three things survive: files in this \
         workspace (shared with every seat), what you post to the room, and the \
         desk memory. Before you post, write your working code and your notes to \
         files — NOTES.md for what you established, and named .py files for code \
         another seat can run — and say in your message which files you wrote.\n\n\
         Rules of the room:\n\
         - Exactly one seat speaks per message. Mentioning a teammate with @id runs \
           their turn next, and only the FIRST @mention in your message does that. \
           Everything after it is read as context.\n\
         - So: end with the one seat you actually need, and put it first among \
           your mentions.\n\
         - Never claim a number you did not compute. Say what you ran.\n\
         - Work as long as the problem needs; run as many tools as it takes. But \
           you must finish by posting: a turn that never posts is a turn the \
           room never happened, and the work in it reaches nobody.\n\
         - To ask one peer something without spending the room's attention on \
           it, make `!aside @peer` the first line of your post. `!surface` then \
           what the room needs to know ends it. An aside counts for nothing \
           until you surface it.\n\
         - Wrap the message you want posted in <<<POST and POST>>>. Anything \
           outside those markers is not posted.\n",
    );
    prompt
}
