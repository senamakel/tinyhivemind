//! One seat's last mile: how a turn's prompt becomes a line of text.
//!
//! Two backends, one prompt. [`Backend::Http`] posts to an `OpenAI`-shaped
//! chat endpoint — the intended target is a local ladder router serving
//! `flash` — and [`Backend::Command`] shells out to an agent CLI that reads a
//! prompt as its final argument, which covers `opencode run`, `claude -p` and
//! `codex exec`. Which one a seat is on changes nothing the library sees: a
//! turn is a string either way, and it is routed by the same mention grammar
//! and the same dispatch fold.
//!
//! Every HTTP request goes through the `curl` binary rather than an HTTP
//! crate. This workspace forbids a transport dependency in `tinyhivemind`, and an
//! example is built alongside it. The whole request — headers included — is
//! written to `curl`'s stdin with `--config -`, so neither the API key nor the
//! body ever appears in the process argument list, where any local process can
//! read it out of `ps`.

use std::fmt;
use std::io::Write as _;
use std::process::{Command, Stdio};

use serde_json::{Value, json};
use tinyhivemind::{SessionAuthor, SessionMessage};

/// Whether the endpoint is asked to think before it answers.
///
/// It is worth a flag because the two regimes cost about an order of
/// magnitude apart on the same prompt, and because the cheap one is not
/// merely cheaper: a router that spends its whole completion budget reasoning
/// its way to a one-line answer returns an *empty* content field, which looks
/// exactly like a broken endpoint. The default is [`Thinking::Off`] for that
/// reason — this harness asks for one line, and one line does not need a
/// scratchpad.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Thinking {
    /// Send no thinking directive; the endpoint's own default applies.
    On,
    /// Ask the endpoint not to think before answering.
    Off,
}

impl Thinking {
    /// Parse a `--thinking` flag's value.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "on" => Some(Self::On),
            "off" => Some(Self::Off),
            _ => None,
        }
    }
}

/// Where a seat's answer comes from.
#[derive(Clone)]
pub(crate) enum Backend {
    /// An `OpenAI`-shaped `/v1/chat/completions` endpoint.
    Http {
        /// Base URL, without a trailing slash or version path.
        base: String,
        /// Bearer token, read from an environment variable and never logged.
        key: String,
        /// Model id the endpoint understands.
        model: String,
        /// Per-request deadline in seconds.
        timeout_secs: u64,
        /// Whether the endpoint is asked to think first.
        thinking: Thinking,
    },
    /// An agent CLI taking the prompt as its final argument.
    Command {
        /// The command and its flags, already split on whitespace.
        argv: Vec<String>,
    },
}

/// Written by hand so a key cannot reach a log through a `{:?}`.
impl fmt::Debug for Backend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Http {
                base,
                model,
                timeout_secs,
                thinking,
                ..
            } => formatter
                .debug_struct("Http")
                .field("base", base)
                .field("key", &"<redacted>")
                .field("model", model)
                .field("timeout_secs", timeout_secs)
                .field("thinking", thinking)
                .finish(),
            Self::Command { argv } => formatter
                .debug_struct("Command")
                .field("argv", argv)
                .finish(),
        }
    }
}

impl Backend {
    /// A short label naming this backend in the run header.
    pub(crate) fn label(&self) -> String {
        match self {
            Self::Http {
                base,
                model,
                thinking,
                ..
            } => format!("http {base} model={model} thinking={thinking:?}"),
            Self::Command { argv } => format!("cmd {}", argv.join(" ")),
        }
    }
}

/// One seat on the desk.
#[derive(Debug)]
pub(crate) struct Seat {
    /// Canonical agent id, matching a roster member and a desk member.
    pub(crate) id: String,
    /// What this seat is for, stated to the model and to the selector.
    pub(crate) role: String,
    /// How this seat's answers are fetched.
    pub(crate) backend: Backend,
}

/// What the seat is being asked to do this turn.
pub(crate) enum Ask<'a> {
    /// Answer the desk. The operator's instruction is already in the
    /// transcript, so nothing extra is stated.
    Desk,
    /// Answer a peer who addressed this seat by name.
    Addressed {
        /// The peer's canonical id.
        from: &'a str,
        /// Exactly what the peer wrote, as the host committed it.
        content: &'a str,
    },
}

impl Seat {
    /// Run one turn and return the single line the seat wrote.
    ///
    /// `visible` is exactly what `project_session` allowed this turn to see —
    /// never the raw journal. `peers` is the rest of the desk, which is what
    /// makes an `@` mention addressable rather than a guess.
    pub(crate) fn speak(
        &self,
        visible: &[SessionMessage],
        peers: &[&str],
        ask: &Ask<'_>,
    ) -> Result<String, String> {
        let system = self.system_prompt(peers);
        let user = user_prompt(visible, ask);
        let reply = match &self.backend {
            Backend::Http {
                base,
                key,
                model,
                timeout_secs,
                thinking,
            } => http_turn(base, key, model, *timeout_secs, *thinking, &system, &user),
            Backend::Command { argv } => command_turn(argv, &format!("{system}\n\n{user}")),
        }?;
        Ok(first_line(&reply))
    }

    fn system_prompt(&self, peers: &[&str]) -> String {
        let roster = if peers.is_empty() {
            "You are the only agent on this desk.".to_owned()
        } else {
            format!(
                "The other agents on this desk are: {}.",
                peers
                    .iter()
                    .map(|id| format!("@{id}"))
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        };
        format!(
            "You are @{}, the {} on the {} desk. {roster}\n\n{RULES}",
            self.id,
            self.role,
            super::host::DESK_NAME,
        )
    }
}

/// What every seat is told about addressing a peer.
///
/// The one-line limit is the harness's, not the library's: it keeps the
/// transcript readable and keeps a run cheap. The rule that matters is the
/// last one — the desk routes on the *first* `@id`, so a line that name-drops
/// two peers reaches only the first, and a line that mentions nobody ends the
/// chain. Stating that plainly is a host obligation: the library will not
/// invent a target for a message that addresses everybody vaguely.
const RULES: &str = "\
Reply with ONE short line and nothing else.

You are talking in a shared desk transcript that every agent here reads. To \
hand the question to a specific peer, write their @id somewhere in your line \
and say what you want from them. The desk routes on the FIRST @id in your \
line and on nothing else, so name the one peer you actually mean. Do not \
write @everyone or @here: those address the room and start no turn, and the \
conversation stops there. If you have the answer yourself, write it and \
mention nobody — that also ends the chain, which is correct when the work is \
done.";

fn user_prompt(visible: &[SessionMessage], ask: &Ask<'_>) -> String {
    let transcript = if visible.is_empty() {
        "(nothing yet)".to_owned()
    } else {
        visible
            .iter()
            .map(|message| {
                format!(
                    "[{}] {}: {}",
                    message.sequence,
                    author_label(&message.author),
                    message.content,
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    match ask {
        Ask::Desk => format!("What this desk can see:\n{transcript}\n\nYour turn."),
        Ask::Addressed { from, content } => format!(
            "What this desk can see:\n{transcript}\n\n\
             @{from} addressed you directly with: {content}\n\nYour turn."
        ),
    }
}

/// The label a transcript line is attributed to.
///
/// Attribution is the whole point of the projection: a seat reading this sees
/// who said what, rather than a flattened history in which every peer's words
/// look like its own.
pub(crate) fn author_label(author: &SessionAuthor) -> String {
    match author {
        SessionAuthor::Operator => "operator".to_owned(),
        SessionAuthor::Person { label, .. } => format!("{label} (human)"),
        SessionAuthor::Agent { id, .. } => format!("@{id}"),
        SessionAuthor::System { label, .. } => label.clone(),
    }
}

/// Keep the first non-blank line, so a chatty backend still yields one turn.
fn first_line(reply: &str) -> String {
    reply
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_owned()
}

/// Post one chat completion and return the assistant's content.
#[allow(clippy::too_many_arguments)]
pub(crate) fn http_turn(
    base: &str,
    key: &str,
    model: &str,
    timeout_secs: u64,
    thinking: Thinking,
    system: &str,
    user: &str,
) -> Result<String, String> {
    let mut body = json!({
        "model": model,
        "temperature": 0,
        "max_tokens": 400,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
    });
    if thinking == Thinking::Off {
        body["thinking"] = json!({"type": "disabled"});
    }
    let body = body.to_string();

    let mut child = Command::new("curl")
        .args(["--config", "-", "--data-binary", &body])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "this harness requires the curl binary".to_owned())?;
    let config = format!(
        "url = \"{base}/v1/chat/completions\"\n\
         request = \"POST\"\n\
         header = \"Authorization: Bearer {key}\"\n\
         header = \"Content-Type: application/json\"\n\
         max-time = {timeout_secs}\n\
         silent\n\
         show-error\n\
         fail-with-body\n",
    );
    child
        .stdin
        .take()
        .ok_or_else(|| "failed to open curl input".to_owned())?
        .write_all(config.as_bytes())
        .map_err(|_| "failed to configure the request".to_owned())?;
    let output = child
        .wait_with_output()
        .map_err(|_| "the request failed to run".to_owned())?;
    if !output.status.success() {
        return Err(format!(
            "the endpoint rejected the request: {}",
            String::from_utf8_lossy(&output.stdout).trim(),
        ));
    }
    let payload: Value = serde_json::from_slice(&output.stdout)
        .map_err(|_| "the endpoint returned invalid JSON".to_owned())?;
    payload["choices"][0]["message"]["content"]
        .as_str()
        .map(|content| content.trim().to_owned())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| {
            // Empty content is usually a budget the endpoint spent reasoning
            // rather than a broken endpoint, so say which and say what to do.
            let finish = payload["choices"][0]["finish_reason"]
                .as_str()
                .unwrap_or("unknown");
            format!(
                "the endpoint returned no assistant content (finish_reason {finish}); \
                 a reasoning model can spend its whole completion budget before \
                 writing a line — try --thinking off"
            )
        })
}

/// Run one agent CLI with the prompt as its final argument.
fn command_turn(argv: &[String], prompt: &str) -> Result<String, String> {
    let (program, flags) = argv
        .split_first()
        .ok_or_else(|| "an empty --agent-cmd runs nothing".to_owned())?;
    let output = Command::new(program)
        .args(flags)
        .arg(prompt)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|_| format!("failed to run {program}"))?;
    if !output.status.success() {
        return Err(format!("{program} exited with {}", output.status));
    }
    let reply = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if reply.is_empty() {
        return Err(format!("{program} printed nothing"));
    }
    Ok(reply)
}
