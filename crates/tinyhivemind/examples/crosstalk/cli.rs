//! Command-line options for the crosstalk harness, and how they are parsed.
//!
//! Two backends are alternatives, never a pair: `--api-base` for an
//! `OpenAI`-shaped endpoint, or `--agent-cmd` for a CLI. [`Options::parse`]
//! rejects giving both, and rejects giving neither.

use tinyhivemind::SESSION_WINDOW;

use crate::agent::{Backend, Thinking};

/// The default opening instruction.
const INSTRUCTION: &str = "We need to move the payments table to the new schema this week. \
Work out how, between you, and tell me the plan and its worst failure mode.";

/// Everything the run was configured with.
#[derive(Debug)]
pub(crate) struct Options {
    /// Which backend runs the seats and, when possible, the selector.
    pub(crate) backend: Backend,
    /// The host hop budget for agent-to-agent dispatch.
    pub(crate) hops: u32,
    /// What the operator posts to open the desk.
    pub(crate) instruction: String,
    /// Whether the agents' exchange runs in a thread rooted at the
    /// instruction, rather than on the desk channel directly.
    pub(crate) thread: bool,
    /// How many messages are projected into one turn.
    pub(crate) window: usize,
    /// Whether an agent may address one peer privately with `!aside @peer`.
    pub(crate) asides: bool,
}

impl Options {
    /// Parse the harness's command-line arguments.
    ///
    /// # Errors
    ///
    /// Returns a message naming the flag when a value is missing, malformed,
    /// or the two backend flags are given together or not at all.
    pub(crate) fn parse(args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut base = None;
        let mut key_env = "LADDER_API_KEY".to_owned();
        let mut model = "flash".to_owned();
        let mut agent_cmd = None;
        let mut timeout_secs = 120_u64;
        let mut thinking = Thinking::Off;
        let mut hops = 3_u32;
        let mut instruction = INSTRUCTION.to_owned();
        let mut thread = false;
        let mut window = SESSION_WINDOW;
        let mut asides = false;

        let mut args = args.peekable();
        while let Some(argument) = args.next() {
            match argument.as_str() {
                "--api-base" => base = Some(next(&mut args, "--api-base")?),
                "--api-key-env" => key_env = next(&mut args, "--api-key-env")?,
                "--model" => model = next(&mut args, "--model")?,
                "--agent-cmd" => agent_cmd = Some(next(&mut args, "--agent-cmd")?),
                "--timeout" => timeout_secs = number(&mut args, "--timeout")?,
                "--thinking" => {
                    let value = next(&mut args, "--thinking")?;
                    thinking = Thinking::parse(&value)
                        .ok_or_else(|| format!("--thinking takes on or off, not {value}"))?;
                }
                "--hops" => {
                    hops = u32::try_from(number(&mut args, "--hops")?)
                        .map_err(|_| "--hops is too large".to_owned())?;
                }
                "--instruction" => instruction = next(&mut args, "--instruction")?,
                "--thread" => thread = true,
                "--aside" => asides = true,
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
                    thinking,
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
            asides,
        })
    }
}

/// Consume and return the next argument, or an error naming `flag`.
fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next().ok_or_else(|| format!("{flag} needs a value"))
}

/// Consume the next argument and parse it as a number, or an error naming
/// `flag`.
fn number(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<u64, String> {
    next(args, flag)?
        .parse()
        .map_err(|_| format!("{flag} needs a number"))
}
