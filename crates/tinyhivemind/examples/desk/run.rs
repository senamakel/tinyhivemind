//! The desk loop: open the room, choose who answers, run a turn, post it,
//! route the reply — until the room closes.
//!
//! One function on purpose. This is the host written out as one story — open
//! the desk, choose who answers, run a turn, post it, route the reply — and
//! the value of an example host is that a reader can follow that order
//! without chasing five helpers. `crosstalk` makes the same trade with
//! `too_many_arguments`.

use std::{
    collections::HashMap,
    fs,
    sync::PoisonError,
    time::Duration,
};

use tinyhivemind::{
    BrevityPolicy, BriefedTeammate, Conversation, MentionDispatchOutcome, SessionAuthor,
    SessionQuery, TeamBriefing,
    aside::{Audience, Viewer},
    desk::{Desk, DeskSet, ResponderMode},
    dispatch::{
        DispatchConversation, DispatchKey, MentionDispatchInput, MentionDispatchPolicy,
        dispatch_mention,
    },
    initialize_session,
    mention::{MentionAuthor, resolve},
    responder::{ResponderRequest, SelectionPolicy, choose_responder},
    roster::{Person, Roster, RosterMember},
    sharing::{SharingPlan, SharingQuery, SharingState, initialized_state, prepare_delta},
};

use crate::{
    BoxError, agent, aside, chat, cli::Options, deskfile, log, memory, prompt::compose_prompt,
    queue::{DeskQueue, PendingTurn},
};

/// How many times one turn may be restarted after a stalled stream.
const STALL_RESTARTS: usize = 1;

/// How long a seat gets to land its work: write it down, then speak.
///
/// A seat that spends its whole working budget inside tool calls has both
/// said nothing *and* left nothing behind, so the next seat starts from an
/// empty directory. This second phase runs in the seat's own session with its
/// tools still attached, which is what a tool-less wrap-up cannot do: it can
/// summarize a turn but it cannot save one.
const LANDING_TIMEOUT: Duration = Duration::from_secs(720);

/// How long a seat gets to write the message it never got round to writing.
const WRAP_UP_TIMEOUT: Duration = Duration::from_secs(600);

// One function on purpose. This is the host written out as one story — open the
// desk, choose who answers, run a turn, post it, route the reply — and the
// value of an example host is that a reader can follow that order without
// chasing five helpers. `crosstalk` makes the same trade with
// `too_many_arguments`.
#[allow(clippy::too_many_lines)]
pub(crate) async fn run(options: Options) -> Result<(), BoxError> {
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
        Some(
            options
                .transcript
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join("desk-raw"),
        ),
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
        tinyhivemind::SessionAuthor::Person {
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
    let mut since_chair = 0_usize;
    let mut tokens = 0_u64;
    while turns < options.max_turns {
        // The chair speaks on a cadence, not only when the room falls silent.
        // A chain of two seats naming each other never goes quiet, so a purely
        // reactive chair never gets a word in — and this desk spent six turns
        // refining a settled fact while the one open implementation step went
        // unbuilt, with nobody whose job it was to say so.
        let due = options.chair_every > 0 && since_chair >= options.chair_every;
        let job = if due {
            None
        } else {
            queue
                .pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .pop_front()
        };
        let job = if let Some(job) = job {
            job
        } else {
            {
                // The chain stopped. The chair either closes the desk or nudges
                // somebody: whose turn it is when nobody was mentioned is a host
                // policy, not something the library decides.
                //
                // It goes back to whoever spoke last, not round-robin to the
                // next seat. A chain dies most often because a seat ran out of
                // time mid-task and its landed message named nobody — and that
                // seat is exactly the one holding the unfinished work. Handing
                // the turn to a different seat there costs a full turn of
                // re-orientation and loses the thread. Round-robin is the
                // fallback for a room where nobody has spoken yet.
                if rounds >= options.rounds {
                    println!("-- chain empty and rounds exhausted; closing the desk");
                    break;
                }
                rounds += 1;
                since_chair = 0;
                let spoken: Vec<String> = transcript
                    .rows()
                    .into_iter()
                    .filter_map(|row| match row.author {
                        SessionAuthor::Agent { id, .. } => Some(id),
                        _ => None,
                    })
                    .collect();
                // Alternate. Even rounds go back to whoever spoke last, because
                // a chain usually dies with that seat holding the unfinished
                // work. Odd rounds go to a seat that has never spoken, because
                // one-message-one-turn lets a pair that keeps naming each other
                // run a four-seat desk between them: over sixteen turns here,
                // two seats spoke and the other two never did. A desk whose
                // checker never checks is not a desk.
                let starved = spec
                    .agents
                    .iter()
                    .find(|seat| !spoken.iter().any(|who| who == &seat.id));
                let seat = match (rounds % 2, starved) {
                    (1, Some(seat)) => seat,
                    _ => spoken
                        .last()
                        .and_then(|id| spec.agent(id))
                        .unwrap_or_else(|| {
                            let seat = &spec.agents[next_seat % spec.agents.len()];
                            next_seat += 1;
                            seat
                        }),
                };
                let nudge = format!(
                    "@{} the room has gone quiet — round {rounds}. Read NOTES.md and the \
                     transcript, then post one concrete step or result and name the seat \
                     you need next. If the room has been round the same loop twice, say so \
                     and change what it is doing.",
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
        since_chair += 1;

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
            asides: aside::ASIDES,
        };
        // Off by default, and that default is the finding. Resuming a seat's
        // CLI session looks like free continuity, but the session keeps every
        // prior turn: the request payload grows without bound until a single
        // call takes minutes and then never returns at all. The room got
        // slower turn by turn and finally stalled on every one. A fresh
        // session answers at once, and the context a seat actually needs is
        // the bounded projection this host already assembles plus the files
        // in the shared workspace — which is what the library's own
        // `prepare_delta` is for.
        let resumed = options
            .resume_sessions
            .then(|| sessions.get(&seat.id).cloned())
            .flatten();
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
        if output.timed_out || output.stalled {
            // The process was killed, so its session is spent. A killed
            // session cannot be resumed — the next run on it exits at once
            // having emitted nothing — so keeping the id would make every
            // later turn resume into silence. That is what turned a room
            // which had been working into one that stalled on every turn.
            sessions.remove(&seat.id);
        } else if let Some(id) = output.session.clone() {
            sessions.insert(seat.id.clone(), id);
        }
        if let Some(error) = output.error.clone() {
            // Say so. An upstream failure that reads as silence is how an hour
            // goes into diagnosing a model that was never asked.
            println!("   !! agent error: {}", &error[..error.len().min(180)]);
            if output.message.trim().is_empty() {
                println!("   retrying the turn once");
                output = runner.run(
                    &prompt,
                    &format!("turn-{turns:03}-{}-retry", seat.id),
                    runner.timeout(),
                    output.session.as_deref().or(resumed.as_deref()),
                )?;
                tokens += output.tokens;
            }
        }
        // One hung stream should cost a couple of minutes, not the turn. Each
        // restart is a fresh session on the same prompt; the seat's earlier
        // work is in the workspace, so what a restart repeats is orientation
        // rather than the work itself.
        let mut restarts = 0;
        while output.stalled && restarts < STALL_RESTARTS {
            restarts += 1;
            println!(
                "   !! stalled after {:?} of silence - restart {restarts}",
                output.elapsed
            );
            output = runner.run(
                &prompt,
                &format!("turn-{turns:03}-{}-restart{restarts}", seat.id),
                runner.timeout(),
                None,
            )?;
            tokens += output.tokens;
        }
        if output.timed_out || output.message.trim().is_empty() {
            // Two phases, because the failure has two halves. First ask the
            // seat to land what it has: same session, tools still attached, so
            // the working code and the notes reach the shared workspace where
            // the next seat can run them. Only if that also runs out of time
            // does the tool-less channel take over, which can make a seat speak
            // but cannot make it save.
            println!(
                "   !! {} after {:?} - asking the seat to land it",
                if output.timed_out {
                    "timed out"
                } else {
                    "silent"
                },
                output.elapsed
            );
            let landing = format!(
                "Stop the investigation here; do not open a new line of work.\n\n\
                 Two things, in order. First write down what this turn established, \
                 so it outlives this process: put your notes in NOTES.md and any \
                 working code in named .py files in this workspace, which every \
                 seat shares. Second, post one message to the room wrapped in \
                 <<<POST and POST>>>, saying what you established, what you did \
                 not finish, which files you wrote, and the one seat you need \
                 next - that seat named first.\n\n{prompt}"
            );
            let mut landed = runner.run(
                &landing,
                &format!("turn-{turns:03}-{}-landing", seat.id),
                LANDING_TIMEOUT,
                output.session.as_deref().or(resumed.as_deref()),
            )?;
            if !landed.posted && landed.tokens == 0 {
                // Nothing at all came back — no events, no complaint. Resuming
                // a session whose process was killed exits silently, and a
                // landing that cannot start is a landing that cannot save. Try
                // again on a fresh session: it loses the seat's working
                // context, which is the lesser of the two losses.
                println!("   landing on the resumed session produced nothing; retrying fresh");
                landed = runner.run(
                    &landing,
                    &format!("turn-{turns:03}-{}-landing-fresh", seat.id),
                    LANDING_TIMEOUT,
                    None,
                )?;
            }
            tokens += landed.tokens;
            if let Some(id) = landed.session.clone() {
                sessions.insert(seat.id.clone(), id);
            }
            let salvage = if !landed.posted || landed.message.trim().is_empty() {
                let wrap = format!(
                    "{prompt}\n\n## What you actually ran this turn\n{}\n\nYou have no \
                     tools. Your working turn ended before you posted anything. Write the \
                     message now, in one block wrapped in <<<POST and POST>>>: what you \
                     established, what you did not finish, and the one seat you need next. \
                     Ground every claim in the commands above; if they do not establish \
                     something, say it is not established.",
                    agent::truncate_work_log(&output.work_log)
                );
                let text = wrapup.complete(&wrap);
                if !text.trim().is_empty() {
                    println!("   wrap-up posted through the router with no tools attached");
                }
                agent::extract_post(&text)
            } else {
                println!("   landed in the seat's own session, files included");
                landed.message
            };
            if salvage.trim().is_empty() {
                println!("   !! nothing salvaged; the seat forfeits this turn");
                continue;
            }
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
        let audience = aside::address(
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
