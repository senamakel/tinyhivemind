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
//! | `--thinking on\|off` | let the endpoint reason first, or not (default `off`) |
//! | `--hops N` | host hop budget for agent-to-agent dispatch (default 3) |
//! | `--instruction TEXT` | what the operator posts to open the desk |
//! | `--thread` | run the agents' exchange in a thread rooted at the instruction |
//! | `--aside` | let an agent address one peer privately, and show what the desk sees instead |
//! | `--window N` | messages projected into one turn (default 30) |
//!
//! # File layout
//!
//! | file | holds |
//! | --- | --- |
//! | `main.rs` | the entry point: parse options, open the desk, print the report |
//! | `cli.rs` | [`cli::Options`] and its parsing |
//! | `selector.rs` | [`selector::LadderSelector`], the model-backed ladder rung, and [`selector::route_opening`] |
//! | `room.rs` | [`room::Room`], the bundle of seats and storage one run holds |
//! | `chain.rs` | [`chain::run_chain`], the hand-off loop, and the aside audience it resolves each turn |
//! | `report.rs` | [`report::Report`], what a run established, and printing it |
//! | `agent.rs` | one seat's last mile: how a prompt becomes a line of text |
//! | `host.rs` | the host side of one desk: journal, queue, and roster |

mod agent;
mod chain;
mod cli;
mod host;
mod report;
mod room;
mod selector;

use std::process::ExitCode;
use std::sync::Arc;

use cli::Options;
use host::{Cast, DESK_ID, DESK_NAME, Journal, OPERATOR_ID, Queue};
use room::Room;
use selector::{LadderSelector, route_opening};

use tinyhivemind::responder::Selector;
use tinyhivemind::{Conversation, SessionAuthor};

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

/// Run one desk end to end: seat the agents, route the opening instruction,
/// run the hand-off chain, and assemble the report.
async fn run(options: &Options) -> Result<report::Report, String> {
    let ids: Vec<&str> = SEATS.iter().map(|(id, _)| *id).collect();
    let cast = Cast::new(&ids);
    let roster = cast.roster();
    let desks = cast.desk_set();

    let seats: Vec<agent::Seat> = SEATS
        .iter()
        .map(|(id, role)| agent::Seat {
            id: (*id).to_owned(),
            role: (*role).to_owned(),
            backend: options.backend.clone(),
        })
        .collect();

    let journal = Arc::new(Journal::default());
    let room = Room {
        seats,
        ids: ids.clone(),
        journal: Arc::clone(&journal),
        queue: Queue::new(Arc::clone(&journal), true, &ids),
        roster,
        desks,
    };

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

    // The model-backed rung of the ladder, on the same endpoint as the seats.
    // A CLI backend gets no selector: `opencode run` answering "which id?"
    // costs a whole process, and the deterministic fallback is the honest
    // behaviour for a host that has no cheap router.
    let selector = LadderSelector {
        backend: options.backend.clone(),
    };
    let selector_ref: Option<&(dyn Selector + '_)> =
        if matches!(options.backend, agent::Backend::Http { .. }) {
            Some(&selector)
        } else {
            None
        };
    let decision = route_opening(options, selector_ref, &SEATS, &room.roster, &room.desks).await?;

    // Where the agents talk. Under `--thread` that is a sub-conversation of
    // the desk, rooted at the operator's message: the same members, the same
    // desk, a narrower projection. Under the default it is the desk channel.
    let floor = Conversation {
        desk_id: DESK_ID.to_owned(),
        desk_name: DESK_NAME.to_owned(),
        thread_root: options.thread.then_some(opening),
    };

    let (turns, refusals) = chain::run_chain(options, &room, &floor, &decision.responder_id).await?;

    let channel_view = report::view(&room.journal, channel, options.window).await?;
    let thread_view = if options.thread {
        report::view(
            &room.journal,
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

    let views = if options.asides {
        report::every_view(&room, &floor, options.window).await?
    } else {
        Vec::new()
    };

    Ok(report::Report {
        asides: options.asides,
        refusals,
        views,
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
