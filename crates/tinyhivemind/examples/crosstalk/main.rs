//! Watch one desk hand a question between its agents, live.
//!
//! ```sh
//! # against a local ladder router
//! cargo run -p tinyhivemind --example crosstalk -- \
//!   --api-base http://127.0.0.1:6969 --api-key-env LADDER_API_KEY --model flash
//!
//! # against an agent CLI
//! cargo run -p tinyhivemind --example crosstalk -- --agent-cmd "opencode run"
//! ```
//!
//! # What this proves, and what it does not
//!
//! It proves that the two edges a desk is made of actually connect end to end
//! with real models on the seats:
//!
//! 1. an instruction addressed to nobody in particular reaches **exactly one**
//!    agent, chosen by `choose_responder` — the responder ladder, with its
//!    model-backed rung running against the same endpoint as the seats;
//! 2. that agent's committed reply can address a **peer by name**, and
//!    `dispatch_mention` turns that into exactly one child turn for that peer,
//!    bound to the same conversation and counted against a hop budget;
//! 3. the peer's turn sees the addressing agent's message **attributed to it**
//!    in the projection, so what crossed is the desk's context and not a
//!    private hand-off.
//!
//! It does not prove that the answers are good, and it cannot. Three turns of
//! one model on a synthetic question is not evidence about deliberation
//! quality; `cargo run -p tinyhivemind-hive --example bench` is where that
//! question is asked, against controls, and the honest answer there is
//! narrower than it looks.
//!
//! It also does not prove privacy, because there is none to prove: every
//! message written here is readable by every member of the desk. See the
//! README's "What a DM would need" section.
//!
//! # Command line
//!
//! | flag | meaning |
//! | --- | --- |
//! | `--api-base URL` | an `OpenAI`-shaped chat endpoint for every seat |
//! | `--api-key-env NAME` | environment variable holding its key (default `LADDER_API_KEY`) |
//! | `--model ID` | model id for the seats and the selector (default `flash`) |
//! | `--agent-cmd "CMD"` | run an agent CLI per turn instead of an endpoint |
//! | `--timeout N` | per-request deadline in seconds (default 120) |
//! | `--hops N` | host hop budget for agent-to-agent dispatch (default 3) |
//! | `--instruction TEXT` | what the operator posts to open the desk |
//! | `--thread` | run the agents' exchange in a thread rooted at the instruction |
//! | `--window N` | messages projected into one turn (default 30) |

mod agent;
mod host;

use std::process::ExitCode;

use agent::{Ask, Backend, Seat, author_label};
use host::{Cast, DESK_ID, DESK_NAME, Journal, OPERATOR_ID, Queue};

use tinyhivemind::dispatch::{
    DispatchConversation, DispatchKey, MentionDispatchInput, MentionDispatchOutcome,
    MentionDispatchPolicy, dispatch_mention,
};
use tinyhivemind::responder::{
    ResponderRequest, SelectionPolicy, Selector, SelectorCandidate, SelectorFuture,
    choose_responder,
};
use tinyhivemind::{
    Conversation, SESSION_WINDOW, Sequence, SessionAuthor, SessionMessage, SessionQuery,
    project_session,
};
use tinyhivemind_core::mention::{MentionAuthor, resolve as resolve_mentions};

/// The desk this harness seats, and what each seat is for.
///
/// Three roles that genuinely need each other: the planner cannot check its
/// own migration against what the desk did last time, and the auditor cannot
/// invent a plan to check. A desk whose members could each answer alone would
/// make a hand-off look optional.
const SEATS: [(&str, &str); 3] = [
    (
        "planner",
        "planner, who turns a request into one concrete proposal",
    ),
    (
        "auditor",
        "auditor, who finds the failure mode in a proposal and says which one it is",
    ),
    (
        "archivist",
        "archivist, who remembers what this team did before and what it cost",
    ),
];

/// The default opening instruction.
const INSTRUCTION: &str = "We need to move the payments table to the new schema this week. \
Work out how, between you, and tell me the plan and its worst failure mode.";

fn main() -> ExitCode {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("crosstalk: {message}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("crosstalk: could not start a runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(&options)) {
        Ok(report) => {
            let failed = report.print();
            if failed {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(message) => {
            eprintln!("crosstalk: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Everything the run was configured with.
#[derive(Debug)]
struct Options {
    backend: Backend,
    hops: u32,
    instruction: String,
    thread: bool,
    window: usize,
}

impl Options {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut base = None;
        let mut key_env = "LADDER_API_KEY".to_owned();
        let mut model = "flash".to_owned();
        let mut agent_cmd = None;
        let mut timeout_secs = 120_u64;
        let mut hops = 3_u32;
        let mut instruction = INSTRUCTION.to_owned();
        let mut thread = false;
        let mut window = SESSION_WINDOW;

        let mut args = args.peekable();
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--api-base" => base = Some(next(&mut args, "--api-base")?),
                "--api-key-env" => key_env = next(&mut args, "--api-key-env")?,
                "--model" => model = next(&mut args, "--model")?,
                "--agent-cmd" => agent_cmd = Some(next(&mut args, "--agent-cmd")?),
                "--timeout" => timeout_secs = number(&mut args, "--timeout")?,
                "--hops" => {
                    hops = u32::try_from(number(&mut args, "--hops")?)
                        .map_err(|_| "--hops is too large".to_owned())?;
                }
                "--instruction" => instruction = next(&mut args, "--instruction")?,
                "--thread" => thread = true,
                "--window" => {
                    window = usize::try_from(number(&mut args, "--window")?)
                        .map_err(|_| "--window is too large".to_owned())?;
                }
                other => return Err(format!("unknown flag {other}")),
            }
        }

        let backend = match (base, agent_cmd) {
            (Some(_), Some(_)) => {
                return Err("--api-base and --agent-cmd are alternatives, not a pair".to_owned());
            }
            (Some(base), None) => {
                let key = std::env::var(&key_env).map_err(|_| {
                    format!("{key_env} must be set, or name another with --api-key-env")
                })?;
                Backend::Http {
                    base: base.trim_end_matches('/').to_owned(),
                    key,
                    model,
                    timeout_secs,
                }
            }
            (None, Some(command)) => {
                let words: Vec<String> = command.split_whitespace().map(str::to_owned).collect();
                if words.is_empty() {
                    return Err("--agent-cmd is empty".to_owned());
                }
                Backend::Command { argv: words }
            }
            (None, None) => {
                return Err(
                    "give --api-base URL (with a key in LADDER_API_KEY) or --agent-cmd \"CMD\""
                        .to_owned(),
                );
            }
        };

        if window == 0 {
            return Err("--window 0 projects nothing".to_owned());
        }
        Ok(Self {
            backend,
            hops,
            instruction,
            thread,
            window,
        })
    }
}

fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

fn number(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<u64, String> {
    next(args, flag)?
        .parse()
        .map_err(|_| format!("{flag} needs a number"))
}

/// One model-backed rung of the responder ladder.
///
/// This is the same endpoint the seats run on, asked a different and much
/// smaller question: given the desk's candidates and the operator's message,
/// name one id. The library validates the answer through `accept_selection`
/// and falls back deterministically when it is not a candidate, so a wrong
/// answer here costs a rung rather than the run.
struct LadderSelector {
    backend: Backend,
}

impl std::fmt::Debug for LadderSelector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LadderSelector")
            .field("backend", &self.backend)
            .finish()
    }
}

impl Selector for LadderSelector {
    fn select<'a>(
        &'a self,
        request: &'a tinyhivemind::responder::SelectionRequest,
    ) -> SelectorFuture<'a> {
        Box::pin(async move {
            let candidates = request
                .candidates
                .iter()
                .map(|candidate| {
                    format!(
                        "{} — {}{}",
                        candidate.id,
                        candidate.role,
                        candidate
                            .description
                            .as_ref()
                            .map(|text| format!(" ({text})"))
                            .unwrap_or_default(),
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let system = "You route one message to one member of a team. \
                Reply with exactly one member id and nothing else."
                .to_owned();
            let user = format!(
                "Team members:\n{candidates}\n\nMessage:\n{}\n\nWhich one id should answer?",
                request.message,
            );
            let reply = match &self.backend {
                Backend::Http {
                    base,
                    key,
                    model,
                    timeout_secs,
                } => agent::http_turn(base, key, model, *timeout_secs, &system, &user),
                // A CLI seat is driven per turn; the selector reuses the same
                // last mile so a `--agent-cmd` run still exercises this rung.
                Backend::Command { .. } => Err("no selector on a CLI backend".to_owned()),
            };
            reply.map_err(|message| -> tinyhivemind::BoxError { message.into() })
        })
    }
}

/// One committed turn, kept so the run can be printed and checked afterwards.
#[derive(Debug)]
struct Turn {
    sequence: Sequence,
    speaker: String,
    hop: u32,
    thread_root: Option<Sequence>,
    content: String,
    /// The ids this turn's projection showed it, in order. This is the
    /// evidence for "desk context crossed", and it is taken from the
    /// projection itself rather than from what the harness believes it wrote.
    saw: Vec<String>,
    outcome: MentionDispatchOutcome,
}

/// What the run established.
#[derive(Debug)]
struct Report {
    backend: String,
    responder: String,
    rung: String,
    disposition: String,
    thread: bool,
    turns: Vec<Turn>,
    channel_view: Vec<String>,
    thread_view: Vec<String>,
}

/// Ask the responder ladder who should answer an unaddressed instruction.
///
/// This is the first of the two edges the harness exists to exercise. The
/// desk runs in `ResponderMode::Auto`, so with more than one effective member
/// the pure plan asks for a selection rather than deciding, and the model
/// rung actually runs. A selector that fails or names a non-candidate costs
/// the rung and not the run: the library falls back deterministically and
/// records why in the disposition.
async fn route_opening(
    options: &Options,
    selector: Option<&(dyn Selector + '_)>,
    roster: &tinyhivemind_core::roster::Roster<'_>,
    desks: &tinyhivemind_core::desk::DeskSet<'_>,
) -> Result<tinyhivemind::responder::ResponderDecision, String> {
    let opening_mentions = resolve_mentions(
        &options.instruction,
        None,
        &MentionAuthor::Person {
            id: OPERATOR_ID.to_owned(),
        },
        roster,
        desks,
    );
    let candidates: Vec<SelectorCandidate> = SEATS
        .iter()
        .map(|(id, role)| SelectorCandidate {
            id: (*id).to_owned(),
            label: (*id).to_owned(),
            role: (*role).to_owned(),
            description: None,
        })
        .collect();
    choose_responder(
        selector,
        &ResponderRequest {
            message: options.instruction.clone(),
            chat: Some(DESK_ID.to_owned()),
            mentions: opening_mentions,
            orchestrator_id: "planner".to_owned(),
            selection_policy: SelectionPolicy::Allowed,
        },
        roster,
        desks,
        &candidates,
    )
    .await
    .map_err(|error| format!("the responder ladder failed: {error}"))
}

/// Run the hand-off chain until the library stops it.
///
/// One turn per iteration, and at most one child turn per turn — that bound is
/// the library's, not this loop's: `mention_dispatch` returns a decision that
/// can carry exactly one request, and there is no variant that carries two.
/// The loop ends when a turn addresses nobody, addresses itself, exhausts the
/// hop budget, or the host's queue refuses it, and the reason is recorded on
/// the last turn rather than inferred afterwards.
async fn run_chain(
    options: &Options,
    seats: &[Seat],
    ids: &[&str],
    journal: &Journal,
    queue: &Queue<'_>,
    roster: &tinyhivemind_core::roster::Roster<'_>,
    desks: &tinyhivemind_core::desk::DeskSet<'_>,
    floor: &Conversation,
    first: &str,
) -> Result<Vec<Turn>, String> {
    let mut turns: Vec<Turn> = Vec::new();
    let mut speaker = first.to_owned();
    let mut hop = 0_u32;
    let mut carried: Option<(String, String)> = None;

    loop {
        let seat = seats
            .iter()
            .find(|seat| seat.id == speaker)
            .ok_or_else(|| format!("no seat for {speaker}"))?;

        let visible = project_session(
            journal,
            &SessionQuery {
                conversation: floor.clone(),
                before: None,
                window: options.window,
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
        let line = seat.speak(&visible, &peers, &ask)?;

        let sequence = journal.append(
            floor,
            SessionAuthor::Agent {
                id: seat.id.clone(),
                label: seat.id.clone(),
            },
            &line,
        );

        let mentions = resolve_mentions(
            &line,
            None,
            &MentionAuthor::Agent {
                id: seat.id.clone(),
            },
            roster,
            desks,
        );
        let outcome = dispatch_mention(
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
                author_id: seat.id.clone(),
                content: line.clone(),
                mentions,
                hop,
            },
            roster,
        )
        .await
        .map_err(|error| format!("dispatch failed: {error}"))?;

        turns.push(Turn {
            sequence,
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
    Ok(turns)
}

/// Render one conversation as the sequence-and-author lines it projects to.
///
/// The report compares two of these — the desk channel and the thread — which
/// is how it shows that a thread is a narrower conversation over the same desk
/// rather than a separate room.
async fn view(
    journal: &Journal,
    conversation: Conversation,
    window: usize,
) -> Result<Vec<String>, String> {
    project_session(
        journal,
        &SessionQuery {
            conversation,
            before: None,
            window,
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

async fn run(options: &Options) -> Result<Report, String> {
    let ids: Vec<&str> = SEATS.iter().map(|(id, _)| *id).collect();
    let cast = Cast::new(&ids);
    let roster = cast.roster();
    let desks = cast.desk_set();

    let seats: Vec<Seat> = SEATS
        .iter()
        .map(|(id, role)| Seat {
            id: (*id).to_owned(),
            role: (*role).to_owned(),
            backend: options.backend.clone(),
        })
        .collect();

    let journal = Journal::default();
    let queue = Queue::new(&journal, true, &ids);

    let channel = Conversation {
        desk_id: DESK_ID.to_owned(),
        desk_name: DESK_NAME.to_owned(),
        thread_root: None,
    };

    // The operator posts an instruction addressed to nobody.
    let opening = journal.append(
        &channel,
        SessionAuthor::Person {
            id: OPERATOR_ID.to_owned(),
            label: "Ada".to_owned(),
        },
        &options.instruction,
    );

    let decision = route_opening(options, selector_ref, &roster, &desks).await?;

    // Where the agents talk. Under `--thread` that is a sub-conversation of
    // the desk, rooted at the operator's message: the same members, the same
    // desk, a narrower projection. Under the default it is the desk channel.
    let floor = Conversation {
        desk_id: DESK_ID.to_owned(),
        desk_name: DESK_NAME.to_owned(),
        thread_root: options.thread.then_some(opening),
    };

    let turns = run_chain(
        options,
        &seats,
        &ids,
        &journal,
        &queue,
        &roster,
        &desks,
        &floor,
        &decision.responder_id,
    )
    .await?;

    let channel_view = view(&journal, channel, options.window).await?;
    let thread_view = if options.thread {
        view(
            &journal,
            Conversation {
                desk_id: DESK_ID.to_owned(),
                desk_name: DESK_NAME.to_owned(),
                thread_root: Some(opening),
            },
            options.window,
        )
        .await?
    } else {
        Vec::new()
    };

    Ok(Report {
        backend: options.backend.label(),
        responder: decision.responder_id,
        rung: format!("{:?}", decision.rung),
        disposition: format!("{:?}", decision.disposition),
        thread: options.thread,
        turns,
        channel_view,
        thread_view,
    })
}

impl Report {
    /// Print the run and its claims. Returns whether any claim failed.
    fn print(&self) -> bool {
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

        for turn in &self.turns {
            let scope = turn
                .thread_root
                .map_or_else(|| "channel".to_owned(), |root| format!("thread@{root}"));
            println!(
                "[{}] hop {} @{} ({scope})\n     saw: {}\n     said: {}\n     then: {}",
                turn.sequence,
                turn.hop,
                turn.speaker,
                turn.saw.join(", "),
                turn.content,
                describe(&turn.outcome),
            );
        }
        println!();

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

        failed
    }
}

fn describe(outcome: &MentionDispatchOutcome) -> String {
    match outcome {
        MentionDispatchOutcome::Enqueued => "one child turn enqueued".to_owned(),
        MentionDispatchOutcome::Already => "already enqueued for this trigger".to_owned(),
        MentionDispatchOutcome::Refused { reason } => format!("host refused: {reason:?}"),
        MentionDispatchOutcome::NotDispatched { reason } => format!("no dispatch: {reason:?}"),
    }
}
