//! What one run of the chain established, and printing it.
//!
//! [`Turn`] is one committed turn kept for the record; [`Report`] is the
//! whole run's evidence, assembled by `main.rs`'s `run` and printed by
//! [`Report::print`], which also checks the claims the harness exists to
//! make and returns whether any of them failed.

use crate::agent::author_label;
use crate::host::{Journal, OPERATOR_ID};
use crate::room::Room;

use std::sync::Arc;

use tinyhivemind::dispatch::MentionDispatchOutcome;
use tinyhivemind::{Conversation, Sequence, SessionMessage, SessionQuery, project_session};
use tinyhivemind_core::aside::{Audience, Viewer};

/// One committed turn, kept so the run can be printed and checked afterwards.
#[derive(Debug)]
pub(crate) struct Turn {
    /// The sequence this turn's row was appended at.
    pub(crate) sequence: Sequence,
    /// Who this row was addressed to: the desk, or a private aside.
    pub(crate) audience: Audience,
    /// The seat that spoke this turn.
    pub(crate) speaker: String,
    /// How many hand-offs already led to this turn.
    pub(crate) hop: u32,
    /// The thread this turn ran in, if any.
    pub(crate) thread_root: Option<Sequence>,
    /// What the seat said.
    pub(crate) content: String,
    /// The ids this turn's projection showed it, in order. This is the
    /// evidence for "desk context crossed", and it is taken from the
    /// projection itself rather than from what the harness believes it wrote.
    pub(crate) saw: Vec<String>,
    /// What the mention-dispatch edge did with this turn's reply.
    pub(crate) outcome: MentionDispatchOutcome,
}

/// What the run established.
#[derive(Debug)]
pub(crate) struct Report {
    /// A human-readable label for the backend the run used.
    pub(crate) backend: String,
    /// The id of the agent the responder ladder chose to open the desk.
    pub(crate) responder: String,
    /// Which rung of the ladder produced that choice.
    pub(crate) rung: String,
    /// The disposition the ladder recorded alongside its choice.
    pub(crate) disposition: String,
    /// Whether the agents' exchange ran in a thread.
    pub(crate) thread: bool,
    /// Every turn the chain took, in order.
    pub(crate) turns: Vec<Turn>,
    /// The desk channel's projection, rendered as sequence-and-author lines.
    pub(crate) channel_view: Vec<String>,
    /// The thread's projection, rendered the same way, when `thread` is set.
    pub(crate) thread_view: Vec<String>,
    /// Whether asides were offered this run.
    pub(crate) asides: bool,
    /// Named refusals the `aside` fold returned, if any.
    pub(crate) refusals: Vec<String>,
    /// One rendered projection per reader, so the run shows what each of them
    /// was actually handed rather than what the journal holds.
    pub(crate) views: Vec<(String, Vec<String>)>,
}

/// What every reader on this desk, and one person, was handed.
///
/// This is the evidence for the whole mechanism: the same rows, at the same
/// sequences, rendered differently for different readers, with a person able
/// to read all of it.
pub(crate) async fn every_view(
    room: &Room<'_>,
    floor: &Conversation,
    window: usize,
) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut views = Vec::new();
    for id in &room.ids {
        views.push((
            format!("@{id}"),
            render_view(
                &room.journal,
                floor.clone(),
                window,
                Viewer::Agent {
                    id: (*id).to_owned(),
                },
            )
            .await?,
        ));
    }
    views.push((
        "Ada (human)".to_owned(),
        render_view(
            &room.journal,
            floor.clone(),
            window,
            Viewer::Person {
                id: OPERATOR_ID.to_owned(),
            },
        )
        .await?,
    ));
    Ok(views)
}

/// Render one conversation as one reader is handed it, line by line.
async fn render_view(
    journal: &Arc<Journal>,
    conversation: Conversation,
    window: usize,
    viewer: Viewer,
) -> Result<Vec<String>, String> {
    project_session(
        journal.as_ref(),
        &SessionQuery {
            conversation,
            before: None,
            window,
            viewer,
        },
    )
    .await
    .map(|messages| messages.iter().map(crate::agent::render).collect())
    .map_err(|error| format!("projection failed: {error}"))
}

/// Render one conversation as the sequence-and-author lines it projects to.
///
/// The report compares two of these — the desk channel and the thread — which
/// is how it shows that a thread is a narrower conversation over the same desk
/// rather than a separate room.
pub(crate) async fn view(
    journal: &Arc<Journal>,
    conversation: Conversation,
    window: usize,
) -> Result<Vec<String>, String> {
    project_session(
        journal.as_ref(),
        &SessionQuery {
            conversation,
            before: None,
            window,
            viewer: Viewer::Operator,
        },
    )
    .await
    .map(|messages| {
        messages
            .iter()
            .map(|message: &SessionMessage| {
                format!("{}:{}", message.sequence, author_label(&message.author))
            })
            .collect::<Vec<_>>()
    })
    .map_err(|error| format!("projection failed: {error}"))
}

impl Report {
    /// Print the run and its claims. Returns whether any claim failed.
    pub(crate) fn print(&self) -> bool {
        println!("backend      {}", self.backend);
        println!(
            "instruction  routed to @{} by {} ({})",
            self.responder, self.rung, self.disposition,
        );
        println!(
            "floor        {}",
            if self.thread {
                "a thread rooted at the operator's message"
            } else {
                "the desk channel"
            },
        );
        println!();

        self.print_transcript();

        if !self.views.is_empty() {
            println!("What each reader was handed:");
            for (who, lines) in &self.views {
                println!("  {who}");
                for line in lines {
                    println!("      {line}");
                }
            }
            println!();
        }
        if !self.refusals.is_empty() {
            println!("aside refusals   {}", self.refusals.join(", "));
            println!();
        }

        if self.thread {
            println!("desk channel sees  {}", self.channel_view.join(", "));
            println!("the thread sees    {}", self.thread_view.join(", "));
            println!();
        }

        let mut failed = false;
        let mut claim = |ok: bool, text: &str| {
            println!("{} {text}", if ok { "PASS" } else { "FAIL" });
            failed |= !ok;
        };

        claim(
            self.turns.len() >= 2,
            "an agent addressed a peer and that peer took a turn",
        );

        let handoffs = self
            .turns
            .iter()
            .filter(|turn| matches!(turn.outcome, MentionDispatchOutcome::Enqueued))
            .count();
        claim(
            handoffs >= 1,
            &format!("{handoffs} agent-to-agent hand-off(s) were dispatched"),
        );

        let saw_the_sender = self.turns.windows(2).all(|pair| {
            let (before, after) = (&pair[0], &pair[1]);
            !matches!(before.outcome, MentionDispatchOutcome::Enqueued)
                || after.saw.iter().any(|line| {
                    line.starts_with(&format!("{}:@{}", before.sequence, before.speaker))
                })
        });
        claim(
            saw_the_sender,
            "every addressed agent saw the addressing message attributed to its author",
        );

        let distinct = {
            let mut seen: Vec<&str> = Vec::new();
            for turn in &self.turns {
                if !seen.contains(&turn.speaker.as_str()) {
                    seen.push(&turn.speaker);
                }
            }
            seen.len()
        };
        claim(
            distinct >= 2,
            &format!("{distinct} distinct agents spoke, so the hand-off left the sender"),
        );

        let ended = self
            .turns
            .last()
            .map_or_else(|| "no turns ran".to_owned(), |turn| describe(&turn.outcome));
        claim(
            self.turns
                .last()
                .is_some_and(|turn| !matches!(turn.outcome, MentionDispatchOutcome::Enqueued)),
            &format!("the chain terminated for a named reason: {ended}"),
        );

        if self.thread {
            claim(
                self.thread_view.len() > self.channel_view.len(),
                "the thread carried the exchange the desk channel only summarises",
            );
        }

        if self.asides {
            self.print_aside_claims(&mut claim);
        }

        failed
    }

    /// The turn-by-turn record: who spoke, what they were shown, what followed.
    fn print_transcript(&self) {
        for turn in &self.turns {
            let scope = turn
                .thread_root
                .map_or_else(|| "channel".to_owned(), |root| format!("thread@{root}"));
            let addressed = if turn.audience.is_desk() {
                String::new()
            } else {
                format!(" → aside with @{}", turn.audience.members().join(", @"))
            };
            println!(
                "[{}] hop {} @{} ({scope}){addressed}\n     saw: {}\n     said: {}\n     then: {}",
                turn.sequence,
                turn.hop,
                turn.speaker,
                turn.saw.join(", "),
                turn.content,
                describe(&turn.outcome),
            );
        }
        println!();
    }

    /// The claims that only an `--aside` run can make.
    fn print_aside_claims(&self, claim: &mut impl FnMut(bool, &str)) {
        {
            let private: Vec<&Turn> = self
                .turns
                .iter()
                .filter(|turn| !turn.audience.is_desk())
                .collect();
            claim(
                !private.is_empty(),
                &format!("{} row(s) were addressed privately", private.len()),
            );

            // The rows are the same rows for everybody, and a member reads
            // what a non-member cannot. Every claim below is bound to the
            // exact row `private.first()` names — a later aside's stub, or a
            // later aside's member, must not be able to satisfy a claim meant
            // for the first one.
            let line_prefix = private.first().map(|turn| format!("[{}", turn.sequence));
            let member_lines = private
                .first()
                .and_then(|turn| {
                    let member = turn.audience.members().first()?;
                    self.views
                        .iter()
                        .find(|(who, _)| who == &format!("@{member}"))
                })
                .map(|(_, lines)| lines.clone())
                .unwrap_or_default();
            let outsider = line_prefix.as_deref().and_then(|prefix| {
                self.views.iter().find_map(|(who, lines)| {
                    if !who.starts_with('@') {
                        return None;
                    }
                    lines
                        .iter()
                        .find(|line| line.starts_with(prefix) && line.contains("· aside,"))
                        .map(|line| (who.clone(), line.clone()))
                })
            });
            claim(
                outsider.is_some(),
                "a non-member was handed a stub instead of the content",
            );
            claim(
                line_prefix.as_deref().is_some_and(|prefix| {
                    !member_lines
                        .iter()
                        .any(|line| line.starts_with(prefix) && line.contains("· aside,"))
                }),
                "the addressed member was handed the content in full",
            );
            let person = self
                .views
                .iter()
                .find(|(who, _)| who.contains("human"))
                .map(|(_, lines)| lines.clone())
                .unwrap_or_default();
            claim(
                !person.is_empty() && !person.iter().any(|line| line.contains("· aside,")),
                "a person read every row in full, so nothing here is unauditable",
            );
            claim(
                outsider.is_some_and(|(_, line)| {
                    line.contains("settled at [") || line.contains("not settled")
                }),
                "the stub says where the aside settled, or that it has not",
            );
        }
    }
}

/// Render a mention-dispatch outcome as the reason it printed for the run.
fn describe(outcome: &MentionDispatchOutcome) -> String {
    match outcome {
        MentionDispatchOutcome::Enqueued => "one child turn enqueued".to_owned(),
        MentionDispatchOutcome::Already => "already enqueued for this trigger".to_owned(),
        MentionDispatchOutcome::Refused { reason } => format!("host refused: {reason:?}"),
        MentionDispatchOutcome::NotDispatched { reason } => format!("no dispatch: {reason:?}"),
    }
}
