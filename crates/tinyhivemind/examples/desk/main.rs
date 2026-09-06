//! A live desk: several real agents sharing one transcript, one turn at a time.
//!
//! This is the host side of `tinyhivemind` written out in full — the parts the
//! library deliberately refuses to own. It opens the storage
//! ([`log::JsonlLog`]), it holds the turn queue, it runs the model
//! ([`agent::AgentRunner`]), and it keeps the memory
//! ([`memory::Memory`]). The library decides *who speaks next* and *what that
//! speaker can see*, and nothing else.
//!
//! ```sh
//! cargo run --release -p tinyhivemind --example desk -- \
//!   --desk crates/tinyhivemind/examples/desk/desks/pe1006.txt \
//!   --task /tmp/pe1006/TASK.md --workspace /tmp/pe1006 --rounds 8
//! ```
//!
//! See `README.md` beside this file for the flags and the failure modes.

mod agent;
mod chat;
mod deskfile;
mod log;
mod memory;

use std::{
    collections::{HashMap, VecDeque},
    error::Error as StdError,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tinyhivemind::{
    BrevityPolicy, BriefedTeammate, Conversation, EnqueueOutcome, MentionDispatchOutcome,
    MentionTurnFuture, MentionTurnQueue, SessionAuthor, SessionMessage, SessionQuery, TeamBriefing,
    aside::{AsideDecision, AsideInput, AsidePolicy, Audience, Viewer, aside},
    desk::{Desk, DeskSet, ResponderMode},
    dispatch::{
        DispatchConversation, DispatchKey, MentionDispatchInput, MentionDispatchPolicy,
        MentionTurnRequest, dispatch_mention,
    },
    initialize_session,
    mention::{Mention, MentionAuthor, MentionTarget, resolve},
    responder::{ResponderRequest, SelectionPolicy, choose_responder},
    roster::{Person, Roster, RosterMember},
    sharing::{SharingPlan, SharingQuery, SharingState, initialized_state, prepare_delta},
};

/// The aside policy this desk runs under.
///
/// A pair, six rows, and a settlement owed before another may be opened. A
/// desk solving one problem wants two seats to be able to sort out a
/// disagreement without spending the room's attention on it — and wants that
/// to end in something the room can read, which is what `must_surface` buys.
const ASIDES: AsidePolicy = AsidePolicy {
    enabled: true,
    max_members: 1,
    max_messages: 6,
    must_surface: true,
    require_thread: false,
};

/// How long a seat gets to write the message it never got round to writing.
const WRAP_UP_TIMEOUT: Duration = Duration::from_secs(600);

/// The error every host-side call in this example returns.
type BoxError = Box<dyn StdError + Send + Sync + 'static>;

/// One turn waiting to run.
#[derive(Clone, Debug)]
struct PendingTurn {
    target_id: String,
    trigger: String,
    hop: u32,
}

/// The host's atomic enqueue boundary: here, a mutex and a queue.
#[derive(Clone, Default)]
struct DeskQueue {
    pending: Arc<Mutex<VecDeque<PendingTurn>>>,
    seen: Arc<Mutex<Vec<(u64, String)>>>,
}

impl MentionTurnQueue for DeskQueue {
    fn enqueue_once(&self, request: MentionTurnRequest) -> MentionTurnFuture<'_> {
        Box::pin(async move {
            let mut seen = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
            let key = (request.key.trigger_sequence, request.target_id.clone());
            if seen.contains(&key) {
                return Ok(EnqueueOutcome::Already);
            }
            seen.push(key);
            drop(seen);
            self.pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push_back(PendingTurn {
                    target_id: request.target_id,
                    trigger: request.content,
                    hop: request.child_hop,
                });
            Ok(EnqueueOutcome::Enqueued)
        })
    }
}

/// Command-line options.
struct Options {
    desk: PathBuf,
    task: PathBuf,
    workspace: PathBuf,
    transcript: PathBuf,
    agent_cmd: String,
    rounds: usize,
    max_turns: usize,
    max_hops: u32,
    window: usize,
    timeout: Duration,
    cortex_base: Option<String>,
    cortex_key: Option<String>,
    library_scope: String,
    session_scope: String,
    opencode_config: Option<String>,
    router_base: String,
    router_key: String,
    router_model: String,
}

impl Options {
    fn parse() -> Result<Self, BoxError> {
        let mut options = Self {
            desk: PathBuf::new(),
            task: PathBuf::new(),
            workspace: PathBuf::from("."),
            transcript: PathBuf::from("transcript.jsonl"),
            agent_cmd: "opencode run --auto --format json -m ladder/reasoning".into(),
            rounds: 8,
            max_turns: 40,
            max_hops: 6,
            window: 40,
            timeout: Duration::from_secs(2400),
            cortex_base: std::env::var("CORTEX_BASE").ok(),
            cortex_key: std::env::var("CORTEX_API_KEY").ok(),
            library_scope: "org:math/problem:euler1006/kind:library".into(),
            session_scope: "org:math/problem:euler1006-hive/kind:session".into(),
            opencode_config: std::env::var("OPENCODE_CONFIG_CONTENT").ok(),
            router_base: std::env::var("LADDER_BASE")
                .unwrap_or_else(|_| "http://127.0.0.1:6969".into()),
            router_key: std::env::var("LADDER_API_KEY").unwrap_or_default(),
            router_model: "deepseek-flash".into(),
        };
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
            match flag.as_str() {
                "--desk" => options.desk = PathBuf::from(value()?),
                "--task" => options.task = PathBuf::from(value()?),
                "--workspace" => options.workspace = PathBuf::from(value()?),
                "--transcript" => options.transcript = PathBuf::from(value()?),
                "--agent-cmd" => options.agent_cmd = value()?,
                "--rounds" => options.rounds = value()?.parse()?,
                "--max-turns" => options.max_turns = value()?.parse()?,
                "--max-hops" => options.max_hops = value()?.parse()?,
                "--window" => options.window = value()?.parse()?,
                "--timeout" => options.timeout = Duration::from_secs(value()?.parse()?),
                "--cortex-base" => options.cortex_base = Some(value()?),
                "--library-scope" => options.library_scope = value()?,
                "--session-scope" => options.session_scope = value()?,
                "--router-base" => options.router_base = value()?,
                "--router-model" => options.router_model = value()?,
                "--no-memory" => {
                    options.cortex_base = None;
                }
                other => return Err(format!("unknown flag {other}").into()),
            }
        }
        if options.desk.as_os_str().is_empty() {
            return Err("--desk is required".into());
        }
        if options.task.as_os_str().is_empty() {
            return Err("--task is required".into());
        }
        Ok(options)
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), BoxError> {
    let options = Options::parse()?;
    let spec = deskfile::parse(&fs::read_to_string(&options.desk)?)?;
    fs::create_dir_all(&options.workspace)?;

    let members: Vec<RosterMember> = spec
        .agents
        .iter()
        .map(|seat| RosterMember {
            id: seat.id.clone(),
            name: Some(seat.label.clone()),
        })
        .collect();
    let people = vec![Person {
        id: spec.person_id.clone(),
        label: spec.person_label.clone(),
    }];
    let roster = Roster::new(&members, &people, &[]);
    let declared = [Desk {
        id: spec.id.clone(),
        name: spec.name.clone(),
        description: None,
        members: spec.agents.iter().map(|seat| seat.id.clone()).collect(),
        responder_mode: ResponderMode::Lead,
    }];
    let desks = DeskSet::new(&declared, &[], &[], &[], &[]);
    roster.validate()?;
    desks.validate()?;

    let conversation = Conversation {
        desk_id: spec.id.clone(),
        desk_name: spec.name.clone(),
        thread_root: None,
    };
    let transcript = log::JsonlLog::open(&options.transcript)?;
    let runner = agent::AgentRunner::new(
        &options.agent_cmd,
        &options.workspace.to_string_lossy(),
        options.opencode_config.clone(),
        options.timeout,
        Some(options.workspace.join(".desk")),
    );
    let store = match (&options.cortex_base, &options.cortex_key) {
        (Some(base), Some(key)) => Some(memory::Memory::new(
            base,
            key,
            &options.library_scope,
            &options.session_scope,
            Duration::from_secs(120),
        )),
        _ => None,
    };
    let wrapup = chat::Chat::new(
        &options.router_base,
        &options.router_key,
        &options.router_model,
        WRAP_UP_TIMEOUT,
    );
    // One CLI session per seat, and one watermark per seat: a seat that has
    // spoken before is caught up with `prepare_delta` rather than re-read the
    // whole window it already holds.
    let mut sessions: HashMap<String, String> = HashMap::new();
    let mut shared: HashMap<String, SharingState> = HashMap::new();
    let queue = DeskQueue::default();
    let policy = MentionDispatchPolicy {
        enabled: true,
        max_hops: options.max_hops,
    };

    // The chair opens the room. A person's message cannot dispatch a turn —
    // only an agent reply can — so the responder ladder chooses who answers it.
    let brief = fs::read_to_string(&options.task)?;
    let mut sequence = transcript.append(
        Some(spec.id.clone()),
        SessionAuthor::Person {
            id: spec.person_id.clone(),
            label: spec.person_label.clone(),
        },
        &brief,
        Audience::Desk,
    )?;
    println!("[{sequence:?}] {} opened the desk", spec.person_label);

    let opening_mentions = resolve(
        &brief,
        None,
        &MentionAuthor::Person {
            id: spec.person_id.clone(),
        },
        &roster,
        &desks,
    );
    let decision = choose_responder(
        None,
        &ResponderRequest {
            message: brief.clone(),
            chat: Some(spec.id.clone()),
            mentions: opening_mentions,
            orchestrator_id: spec.agents[0].id.clone(),
            selection_policy: SelectionPolicy::Disabled,
        },
        &roster,
        &desks,
        &[],
    )
    .await?;
    println!(
        "  responder ladder: {} via {:?}",
        decision.responder_id, decision.rung
    );
    queue
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push_back(PendingTurn {
            target_id: decision.responder_id,
            trigger: brief.clone(),
            hop: 0,
        });

    let mut turns = 0_usize;
    let mut rounds = 0_usize;
    let mut next_seat = 0_usize;
    let mut tokens = 0_u64;
    while turns < options.max_turns {
        let job = queue.pending.lock().unwrap_or_else(PoisonError::into_inner).pop_front();
        let job = match job {
            Some(job) => job,
            None => {
                // The chain stopped. The chair either closes the desk or nudges
                // the next seat: whose turn it is when nobody was mentioned is
                // a host policy, not something the library decides.
                if rounds >= options.rounds {
                    println!("-- chain empty and rounds exhausted; closing the desk");
                    break;
                }
                rounds += 1;
                let seat = &spec.agents[next_seat % spec.agents.len()];
                next_seat += 1;
                let nudge = format!(
                    "@{} the room has gone quiet — round {rounds}. Post your next concrete \
                     step or result, and name the seat you need next.",
                    seat.id
                );
                sequence = transcript.append(
                    Some(spec.id.clone()),
                    SessionAuthor::Person {
                        id: spec.person_id.clone(),
                        label: spec.person_label.clone(),
                    },
                    &nudge,
                    Audience::Desk,
                )?;
                println!("[{}] chair nudge -> @{}", sequence.0, seat.id);
                PendingTurn {
                    target_id: seat.id.clone(),
                    trigger: nudge,
                    hop: 0,
                }
            }
        };

        let Some(seat) = spec.agent(&job.target_id) else {
            println!("!! no seat named {}", job.target_id);
            continue;
        };
        turns += 1;

        let viewer = Viewer::Agent {
            id: seat.id.clone(),
        };
        let query = SessionQuery {
            conversation: conversation.clone(),
            before: None,
            window: options.window,
            viewer: viewer.clone(),
        };
        let briefing = TeamBriefing {
            viewer_id: seat.id.clone(),
            desk_id: spec.id.clone(),
            desk_name: spec.name.clone(),
            teammates: spec
                .agents
                .iter()
                .filter(|other| other.id != seat.id)
                .map(|other| BriefedTeammate {
                    id: other.id.clone(),
                    label: other.label.clone(),
                    role: Some(other.role.clone()),
                    description: None,
                })
                .collect(),
            brevity: BrevityPolicy::DEFAULT,
            asides: ASIDES,
        };
        let resumed = sessions.get(&seat.id).cloned();
        let plan = match (resumed.as_ref(), shared.get(&seat.id)) {
            (Some(_), Some(state)) => Some(
                prepare_delta(
                    &transcript,
                    &SharingQuery {
                        desired_conversation: &conversation,
                        current_conversation: &conversation,
                        state,
                        before: sequence,
                        viewer: &viewer,
                    },
                )
                .await?,
            ),
            _ => None,
        };
        let (history, briefing_text, catching_up) = if let Some(SharingPlan::Delta(delta)) = plan {
            shared.insert(seat.id.clone(), delta.next_state);
            (delta.messages, None, true)
        } else {
                let session = initialize_session(&transcript, &query, briefing).await?;
                shared.insert(
                    seat.id.clone(),
                    initialized_state(conversation.clone(), sequence),
                );
                let text = session.briefing.system_text();
                (session.history, Some(text), false)
            }
        };
        let recalled = store
            .as_ref()
            .map(|store| store.recall(&job.trigger))
            .unwrap_or_default();

        let prompt = compose_prompt(briefing_text.as_deref(), &history, seat, &job, &recalled);
        println!(
            "[turn {turns}] @{} ({} chars of prompt, {} {} message(s){})",
            seat.id,
            prompt.len(),
            history.len(),
            if catching_up { "new" } else { "of history" },
            if resumed.is_some() { ", resumed" } else { "" }
        );
        let mut output = runner.run(
            &prompt,
            &format!("turn-{turns:03}-{}", seat.id),
            runner.timeout(),
            resumed.as_deref(),
        )?;
        tokens += output.tokens;
        if let Some(id) = output.session.clone() {
            sessions.insert(seat.id.clone(), id);
        }
        if output.timed_out || output.message.trim().is_empty() {
            // A seat that spent its whole budget inside tool calls has said
            // nothing, and a room cannot read work it was never told about.
            // One short, tool-less turn to make it speak is a host obligation:
            // the library has no way to know a process was killed.
            println!(
                "   !! {} after {:?} — asking for a wrap-up",
                if output.timed_out {
                    "timed out"
                } else {
                    "silent"
                },
                output.elapsed
            );
            let wrap = format!(
                "{prompt}\n\n## What you actually ran this turn\n{}\n\nYou have no tools. Your working turn ended before you posted \
                 anything. Write the message now, in one block wrapped in <<<POST and \
                 POST>>>: what you established, what you did not finish, and the one seat \
                 you need next. Ground every claim in the commands above; if they \
                 do not establish something, say it is not established.",
                agent::truncate_work_log(&output.work_log)
            );
            let salvage = wrapup.complete(&wrap);
            let salvage = agent::extract_post(&salvage);
            if salvage.trim().is_empty() {
                println!("   !! wrap-up also silent; the seat forfeits this turn");
                continue;
            }
            println!("   wrap-up posted through the router with no tools attached");
            output.message = salvage;
        }
        println!(
            "   {:?}, {} tokens, tools: {}",
            output.elapsed,
            output.tokens,
            if output.tools.is_empty() {
                "none".to_string()
            } else {
                output.tools.join(",")
            }
        );
        for line in output.message.lines().take(6) {
            println!("   | {line}");
        }

        let mentions = resolve(
            &output.message,
            None,
            &MentionAuthor::Agent {
                id: seat.id.clone(),
            },
            &roster,
            &desks,
        );
        // Who the line reaches is the library's decision, not this host's
        // reading of the marker: `aside` resolves the audience and refuses
        // with a named reason, and a refusal leaves the row desk-visible.
        let audience = address(
            &transcript,
            &spec.id,
            &seat.id,
            &output.message,
            &mentions,
            &roster,
            &desks,
        )?;
        if let Audience::Aside { members } = &audience {
            println!("   aside to @{}", members.join(", @"));
        }
        sequence = transcript.append(
            Some(spec.id.clone()),
            SessionAuthor::Agent {
                id: seat.id.clone(),
                label: seat.label.clone(),
            },
            &output.message,
            audience,
        )?;
        if let Some(store) = store.as_ref() {
            store.capture(&seat.id, sequence.0, &output.message);
        }

        let outcome = dispatch_mention(
            &queue,
            policy,
            &MentionDispatchInput {
                key: DispatchKey {
                    trigger_sequence: sequence.0,
                },
                conversation: DispatchConversation {
                    desk_id: spec.id.clone(),
                    thread_root: None,
                },
                author_id: seat.id.clone(),
                content: output.message.clone(),
                mentions,
                hop: job.hop,
            },
            &roster,
        )
        .await?;
        match outcome {
            MentionDispatchOutcome::Enqueued => println!("   -> one child turn enqueued"),
            MentionDispatchOutcome::Already => println!("   -> duplicate, not enqueued"),
            MentionDispatchOutcome::Refused { reason } => println!("   -> refused: {reason:?}"),
            MentionDispatchOutcome::NotDispatched { reason } => {
                println!("   -> no child turn: {reason:?}");
            }
        }
    }

    println!(
        "\ndesk closed: {turns} turns, {} rows, {tokens} tokens reported",
        transcript.len()
    );
    Ok(())
}

/// Assemble everything one seat sees for one turn.
///
/// The order matters: who it is, what the desk knows, what the room has said,
/// then what it was actually asked. A model reads the last thing best.
fn compose_prompt(
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
        use std::fmt::Write as _;
        let _ = write!(prompt, "\n[{}] {who}: {}\n", message.sequence.0, message.content);
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

/// Decide who one authored line is addressed to.
///
/// The harness never acts on the `!aside` marker itself. It hands the line to
/// [`aside`], which resolves who it may reach and refuses with a named reason
/// otherwise; a refusal leaves the row desk-visible, which is the safe
/// direction to fail in.
fn address(
    transcript: &log::JsonlLog,
    desk_id: &str,
    author_id: &str,
    line: &str,
    mentions: &[Mention],
    roster: &tinyhivemind::roster::Roster<'_>,
    desks: &tinyhivemind::desk::DeskSet<'_>,
) -> Result<Audience, BoxError> {
    if !line.trim_start().starts_with("!aside") {
        return Ok(Audience::Desk);
    }
    let rows = transcript.rows();
    let decision = aside(
        ASIDES,
        &AsideInput {
            conversation: DispatchConversation {
                desk_id: desk_id.to_string(),
                thread_root: None,
            },
            author_id: author_id.to_string(),
            mentions: mentions.to_vec(),
            spent: spent_in_aside(&rows),
            unsettled: unsettled_aside(&rows, author_id, mentions),
        },
        roster,
        desks,
    )?;
    Ok(match decision {
        AsideDecision::One { audience } => audience,
        AsideDecision::None { reason } => {
            println!("   aside refused: {reason:?} — the row stays desk-visible");
            Audience::Desk
        }
    })
}

/// How many rows the currently open aside has already spent.
///
/// Folded from the journal rather than stored, because this host keeps no
/// state the journal does not already carry. Every row in the run of
/// consecutive non-desk rows counts, not only the ones one speaker wrote: two
/// peers alternating inside the same aside share one budget.
fn spent_in_aside(rows: &[tinyhivemind::LogMessage]) -> usize {
    rows.iter()
        .rev()
        .take_while(|row| !row.audience.is_desk())
        .count()
}

/// Whether a *different* prior aside among the same participants is unsettled.
///
/// Replying inside the aside already in progress is not opening another one —
/// the budget above is what ends that run — so a line whose immediately
/// preceding row already carries this participant set is a continuation.
/// Walking the journal, an aside opens when its participant set matches and
/// closes the first time one of those participants speaks on the desk.
fn unsettled_aside(
    rows: &[tinyhivemind::LogMessage],
    author_id: &str,
    mentions: &[Mention],
) -> bool {
    let mut participants = addressed(mentions, author_id);
    if participants.is_empty() {
        return false;
    }
    participants.push(author_id.to_string());
    participants.sort();
    participants.dedup();

    if let Some(row) = rows.last()
        && let Audience::Aside { members } = &row.audience
        && let SessionAuthor::Agent { id, .. } = &row.author
    {
        let mut here: Vec<String> = members.clone();
        here.push(id.clone());
        here.sort();
        here.dedup();
        if here == participants {
            return false;
        }
    }

    let mut open = false;
    for row in rows {
        let SessionAuthor::Agent { id, .. } = &row.author else {
            continue;
        };
        match &row.audience {
            Audience::Aside { members } => {
                let mut here: Vec<String> = members.clone();
                here.push(id.clone());
                here.sort();
                here.dedup();
                if here == participants {
                    open = true;
                }
            }
            Audience::Desk => {
                if participants.iter().any(|who| who == id) {
                    open = false;
                }
            }
        }
    }
    open
}

/// The active agent ids a line addresses, without the author and without
/// repeats, in the order they were written.
fn addressed(mentions: &[Mention], author_id: &str) -> Vec<String> {
    let mut ordered: Vec<&Mention> = mentions.iter().collect();
    ordered.sort_by_key(|mention| mention.offset);
    let mut out: Vec<String> = Vec::new();
    for mention in ordered {
        if mention.quiet {
            continue;
        }
        if let MentionTarget::Agent { id } = &mention.target
            && id != author_id
            && !out.contains(id)
        {
            out.push(id.clone());
        }
    }
    out
}
