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
mod deskfile;
mod log;
mod memory;

use std::{
    collections::VecDeque,
    error::Error,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tinyhivemind::{
    BriefedTeammate, BrevityPolicy, Conversation, EnqueueOutcome, MentionDispatchOutcome,
    MentionTurnFuture, MentionTurnQueue, SessionAuthor, SessionMessage, SessionQuery, Sequence,
    TeamBriefing,
    desk::{Desk, DeskSet, ResponderMode},
    dispatch::{
        DispatchConversation, DispatchKey, MentionDispatchInput, MentionDispatchPolicy,
        MentionTurnRequest, dispatch_mention,
    },
    initialize_session,
    mention::{MentionAuthor, resolve},
    responder::{ResponderRequest, SelectionPolicy, choose_responder},
    roster::{Person, Roster, RosterMember},
};

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
            let mut seen = self.seen.lock().expect("queue lock");
            let key = (request.key.trigger_sequence, request.target_id.clone());
            if seen.contains(&key) {
                return Ok(EnqueueOutcome::Already);
            }
            seen.push(key);
            drop(seen);
            self.pending
                .lock()
                .expect("queue lock")
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
}

impl Options {
    fn parse() -> Result<Self, Box<dyn Error>> {
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
            timeout: Duration::from_secs(900),
            cortex_base: std::env::var("CORTEX_BASE").ok(),
            cortex_key: std::env::var("CORTEX_API_KEY").ok(),
            library_scope: "org:math/problem:euler1006/kind:library".into(),
            session_scope: "org:math/problem:euler1006-hive/kind:session".into(),
            opencode_config: std::env::var("OPENCODE_CONFIG_CONTENT").ok(),
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
async fn main() -> Result<(), Box<dyn Error>> {
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
        .expect("queue lock")
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
        let job = queue.pending.lock().expect("queue lock").pop_front();
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

        let query = SessionQuery {
            conversation: conversation.clone(),
            before: None,
            window: options.window,
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
        };
        let session = initialize_session(&transcript, &query, briefing).await?;
        let recalled = store
            .as_ref()
            .map(|store| store.recall(&job.trigger))
            .unwrap_or_default();

        let prompt = compose_prompt(&session.briefing, &session.history, seat, &job, &recalled);
        println!(
            "[turn {turns}] @{} ({} chars of prompt, {} messages of history)",
            seat.id,
            prompt.len(),
            session.history.len()
        );
        let output = runner.run(&prompt)?;
        tokens += output.tokens;
        if output.message.trim().is_empty() {
            println!("   !! empty turn after {:?}", output.elapsed);
            continue;
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

        sequence = transcript.append(
            Some(spec.id.clone()),
            SessionAuthor::Agent {
                id: seat.id.clone(),
                label: seat.label.clone(),
            },
            &output.message,
        )?;
        if let Some(store) = store.as_ref() {
            store.capture(&seat.id, sequence.0, &output.message);
        }

        let mentions = resolve(
            &output.message,
            None,
            &MentionAuthor::Agent { id: seat.id.clone() },
            &roster,
            &desks,
        );
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
    briefing: &TeamBriefing,
    history: &[SessionMessage],
    seat: &deskfile::AgentSpec,
    job: &PendingTurn,
    recalled: &str,
) -> String {
    let mut prompt = briefing.system_text();
    prompt.push_str("\n\n## Your standing brief\n");
    prompt.push_str(seat.brief.trim());
    if !recalled.trim().is_empty() {
        prompt.push_str("\n\n## Desk memory (CortexDB)\n");
        prompt.push_str(recalled.trim());
    }
    prompt.push_str("\n\n## The room so far\n");
    if history.is_empty() {
        prompt.push_str("(empty)\n");
    }
    for message in history {
        let who = match &message.author {
            SessionAuthor::Agent { id, .. } => format!("@{id}"),
            SessionAuthor::Person { label, .. } => label.clone(),
            SessionAuthor::Operator => "operator".to_string(),
            SessionAuthor::System { kind, .. } => format!("system/{kind}"),
        };
        prompt.push_str(&format!("\n[{}] {who}: {}\n", message.sequence.0, message.content));
    }
    prompt.push_str("\n\n## This turn\n");
    prompt.push_str(&format!(
        "You were addressed by this message:\n\n{}\n\n",
        job.trigger.trim()
    ));
    prompt.push_str(
        "Do the work first — use your tools, write and run code in this workspace, check \
         what you claim. Then post ONE message to the room.\n\n\
         Rules of the room:\n\
         - Exactly one seat speaks per message. Mentioning a teammate with @id runs \
           their turn next, and only the FIRST @mention in your message does that. \
           Everything after it is read as context.\n\
         - So: end with the one seat you actually need, and put it first among \
           your mentions.\n\
         - Never claim a number you did not compute. Say what you ran.\n\
         - Wrap the message you want posted in <<<POST and POST>>>. Anything \
           outside those markers is not posted.\n",
    );
    prompt
}
