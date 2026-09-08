//! A live desk: several real agents sharing one transcript, one turn at a time.
//!
//! This is the host side of `tinyhivemind` written out in full — the parts the
//! library deliberately refuses to own. It opens the storage
//! ([`log::JsonlLog`]), it holds the turn queue ([`queue::DeskQueue`]), it
//! runs the model ([`agent::AgentRunner`]), and it keeps the memory
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
//!
//! # File layout
//!
//! | file | holds |
//! | --- | --- |
//! | `main.rs` | the entry point: parse options, run the desk loop |
//! | `cli.rs` | [`cli::Options`] and its parsing |
//! | `run.rs` | [`run::run`], the desk loop itself: one function on purpose |
//! | `queue.rs` | [`queue::DeskQueue`], the host's `MentionTurnQueue` |
//! | `aside.rs` | this desk's aside policy and its host-side bookkeeping |
//! | `prompt.rs` | [`prompt::compose_prompt`], turning a turn into text |
//! | `agent.rs` | one `opencode run` per turn, and its output |
//! | `chat.rs` | the tool-less wrap-up channel |
//! | `deskfile.rs` | parsing the plain-text desk file |
//! | `log.rs` | the JSONL-backed `SessionLog` |
//! | `memory.rs` | CortexDB recall and capture |

mod agent;
mod aside;
mod chat;
mod cli;
mod deskfile;
mod log;
mod memory;
mod prompt;
mod queue;
mod run;

use std::error::Error as StdError;

/// The error every host-side call in this example returns.
type BoxError = Box<dyn StdError + Send + Sync + 'static>;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), BoxError> {
    let options = cli::Options::parse()?;
    run::run(options).await
}
