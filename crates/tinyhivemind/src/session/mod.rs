//! Validated projection over a host-owned session log.

#[cfg(test)]
mod test;

mod types;

pub use types::{
    Conversation, Elision, LogMessage, Sequence, SessionAuthor, SessionMessage, SessionPage,
    SessionQuery,
};

use crate::{Error, Result};
use std::{
    collections::{BTreeSet, HashSet},
    error::Error as StdError,
    future::Future,
    pin::Pin,
};
use tinyhivemind_core::{
    aside::{Audience, Viewer},
    chat::same_conversation,
};

/// Default number of qualifying messages requested for a turn.
pub const SESSION_WINDOW: usize = 30;
/// Maximum raw rows requested in one host read.
pub const PAGE_SIZE: usize = 512;
/// Maximum raw rows inspected during one projection.
pub const SCAN_LIMIT: usize = 2048;

/// A boxed source error returned by a host log implementation.
pub type SourceError = Box<dyn StdError + Send + Sync + 'static>;

/// The boxed future returned by [`SessionLog`].
pub type SessionFuture<'a> =
    Pin<Box<dyn Future<Output = std::result::Result<SessionPage, SourceError>> + Send + 'a>>;

/// Read-only access to the host's append-only session log.
///
/// Implementations must return rows newest-first and treat `before` as an
/// exclusive sequence bound. The trait is object-safe and chooses no executor.
pub trait SessionLog: Send + Sync {
    /// Read at most `limit` raw rows older than `before`.
    ///
    /// # Cost
    ///
    /// This is the hot primitive, and an implementation that reads forward from
    /// the head of the journal to keep the last `limit` rows costs
    /// O(total events) per call. That is not a cold-start cost: it is paid once
    /// per rebind *and* once per watermark tick, so a linear implementation is
    /// linear in the company's whole history on every turn. `opencompany`
    /// measured 72.8ms against 0.4ms at 100k events — linear against flat —
    /// before reading its journal backwards in fixed-size tail chunks.
    ///
    /// Implementations should seek from the tail and parse only the rows they
    /// return. `limit` is bounded by [`PAGE_SIZE`], so a tail read is bounded
    /// too; a forward scan is not.
    fn read_before(&self, before: Option<Sequence>, limit: usize) -> SessionFuture<'_>;
}

/// Project one bounded, attributed history from a host log.
///
/// Reaching [`SCAN_LIMIT`] is a successful partial projection. The returned
/// messages are chronological even though the port pages newest-first.
///
/// A thread-scoped query returns the root and its direct replies. A
/// channel-level query returns every root **and each root's first reply**: an
/// agent reading a desk sees the answers, not a run of unanswered questions,
/// and still never reads a second thread's interior. See
/// [`docs/specs/thread-scoped-conversations.md`][spec].
///
/// [spec]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/specs/thread-scoped-conversations.md
///
/// # Errors
///
/// Returns [`Error::Read`] for a host read failure or a typed page-validation
/// error when a host violates ordering, uniqueness, size, or cursor contracts.
pub async fn project_session(
    log: &(dyn SessionLog + '_),
    query: &SessionQuery,
) -> Result<Vec<SessionMessage>> {
    if query.window == 0 {
        return Ok(Vec::new());
    }
    let projected = match query.conversation.thread_root {
        Some(_) => project_thread(log, query).await?,
        None => project_channel(log, query).await?,
    };
    Ok(collapse_elisions(projected))
}

/// The agent id a row is attributed to, or `None` for any other author.
pub(crate) fn author_agent_id(author: &SessionAuthor) -> Option<&str> {
    match author {
        SessionAuthor::Agent { id, .. } => Some(id.as_str()),
        SessionAuthor::Operator | SessionAuthor::Person { .. } | SessionAuthor::System { .. } => {
            None
        }
    }
}

/// Whether `viewer` may read this row's content.
pub(crate) fn admits(message: &LogMessage, viewer: &Viewer) -> bool {
    message
        .audience
        .admits(viewer, author_agent_id(&message.author))
}

/// Build one projected message, eliding its content when the viewer is not
/// admitted to it.
///
/// The row is kept either way. Dropping it would leave `^N` citations naming
/// nothing, would hide the existence of an exchange the room is entitled to
/// know happened, and would take from a peer the only signal that there is
/// something worth asking about.
pub(crate) fn present(
    sequence: Sequence,
    author: SessionAuthor,
    content: String,
    audience: Audience,
    viewer: &Viewer,
) -> SessionMessage {
    if audience.admits(viewer, author_agent_id(&author)) {
        return SessionMessage {
            sequence,
            author,
            content,
            audience,
            elided: None,
        };
    }
    SessionMessage {
        sequence,
        author,
        content: String::new(),
        audience,
        elided: Some(Elision {
            through: sequence,
            messages: 1,
            settled_at: None,
        }),
    }
}

/// Everyone who can read a given elided row: its author plus its audience.
///
/// Two rows belong to the same aside when these sets are equal, which is what
/// makes an exchange collapse rather than only a monologue: `alice` writing to
/// `bob` and `bob` writing back to `alice` name different audiences and the
/// same participants.
fn participants(message: &SessionMessage) -> HashSet<&str> {
    let mut set: HashSet<&str> = message
        .audience
        .members()
        .iter()
        .map(String::as_str)
        .collect();
    if let Some(id) = author_agent_id(&message.author) {
        set.insert(id);
    }
    set
}

/// Collapse each consecutive run of elided rows from one aside into one stub.
///
/// The stub keeps the run's first sequence, records the last, counts what it
/// stands in for, and points at where the aside settled — the first thing a
/// participant said in the open afterwards, which is by construction a row
/// this viewer can read.
///
/// This runs after the window has been filled, so a viewer with asides in
/// view receives fewer messages than one without. That is a real cost and it
/// is the honest one: the alternative is backfilling from older history, which
/// would make two viewers of the same desk disagree about how far back the
/// window reaches.
pub(crate) fn collapse_elisions(projected: Vec<SessionMessage>) -> Vec<SessionMessage> {
    if !projected.iter().any(|message| message.elided.is_some()) {
        return projected;
    }

    let mut collapsed: Vec<SessionMessage> = Vec::with_capacity(projected.len());
    for message in projected {
        let extends = match (collapsed.last(), &message.elided) {
            (Some(previous), Some(_)) => {
                previous.elided.is_some() && participants(previous) == participants(&message)
            }
            _ => false,
        };
        if extends
            && let Some(previous) = collapsed.last_mut()
            && let Some(elision) = previous.elided.as_mut()
        {
            elision.through = message.sequence;
            elision.messages = elision.messages.saturating_add(1);
            continue;
        }
        collapsed.push(message);
    }

    settle(&mut collapsed);
    collapsed
}

/// Point each stub at the first thing one of its participants said in the open.
fn settle(messages: &mut [SessionMessage]) {
    for index in 0..messages.len() {
        if messages[index].elided.is_none() {
            continue;
        }
        let inside = participants(&messages[index])
            .into_iter()
            .map(str::to_owned)
            .collect::<HashSet<_>>();
        let settled_at = messages[index + 1..]
            .iter()
            .find(|later| {
                later.elided.is_none()
                    && author_agent_id(&later.author).is_some_and(|id| inside.contains(id))
            })
            .map(|later| later.sequence);
        if let Some(elision) = messages[index].elided.as_mut() {
            elision.settled_at = settled_at;
        }
    }
}

async fn project_thread(
    log: &(dyn SessionLog + '_),
    query: &SessionQuery,
) -> Result<Vec<SessionMessage>> {
    let mut cursor = query.before;
    let mut scanned = 0_usize;
    let mut seen = Vec::new();
    let mut projected = Vec::new();
    let mut reached_root = false;

    while scanned < SCAN_LIMIT && projected.len() < query.window && !reached_root {
        let limit = PAGE_SIZE.min(SCAN_LIMIT - scanned);
        let page = log
            .read_before(cursor, limit)
            .await
            .map_err(|source| Error::Read { source })?;
        validate_page(&page, cursor, limit, &mut seen)?;
        scanned += page.messages.len();

        for message in &page.messages {
            if !matches_conversation(message, &query.conversation) {
                continue;
            }
            let is_thread_root = query.conversation.thread_root == Some(message.sequence);
            if message.content.trim().is_empty() {
                if is_thread_root {
                    reached_root = true;
                    break;
                }
                continue;
            }
            projected.push(present(
                message.sequence,
                message.author.clone(),
                message.content.clone(),
                message.audience.clone(),
                &query.viewer,
            ));
            if is_thread_root {
                reached_root = true;
                break;
            }
            if projected.len() == query.window {
                break;
            }
        }

        if reached_root || projected.len() == query.window || scanned == SCAN_LIMIT {
            break;
        }
        let Some(next) = page.next_before else {
            break;
        };
        cursor = Some(next);
    }

    projected.truncate(query.window);
    projected.reverse();
    Ok(projected)
}

/// One in-desk row held with the `parent` [`SessionMessage`] does not carry.
///
/// Picking each root's *first* reply needs the parent link and chronological
/// order, and the walk runs newest-first, so the narrowing cannot be a fold a
/// caller applies to the result.
#[derive(Debug)]
struct Candidate {
    sequence: Sequence,
    parent: Option<Sequence>,
    author: SessionAuthor,
    content: String,
    audience: Audience,
}

async fn project_channel(
    log: &(dyn SessionLog + '_),
    query: &SessionQuery,
) -> Result<Vec<SessionMessage>> {
    let mut cursor = query.before;
    let mut scanned = 0_usize;
    let mut seen = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    // Survivors, counted newest-first while the parent of an older row is still
    // unknown: every non-empty root survives, and every distinct parent
    // contributes at most one promoted reply. A parent that turns out to be a
    // reply, or to sit outside the scan, is counted here and dropped below, so
    // this over-estimates and the walk can stop a little early — never late.
    let mut roots = BTreeSet::new();
    let mut answered = BTreeSet::new();
    let mut window_met = false;

    while scanned < SCAN_LIMIT && !window_met {
        let limit = PAGE_SIZE.min(SCAN_LIMIT - scanned);
        let page = log
            .read_before(cursor, limit)
            .await
            .map_err(|source| Error::Read { source })?;
        validate_page(&page, cursor, limit, &mut seen)?;
        scanned += page.messages.len();

        for message in &page.messages {
            if !in_desk(message, &query.conversation) {
                continue;
            }
            candidates.push(Candidate {
                sequence: message.sequence,
                parent: message.parent,
                author: message.author.clone(),
                content: message.content.clone(),
                audience: message.audience.clone(),
            });
            // An empty row is kept as a candidate so it can still be somebody's
            // root, but it is neither a survivor nor a spent promotion.
            if message.content.trim().is_empty() {
                continue;
            }
            match message.parent {
                None => roots.insert(message.sequence),
                Some(parent) => answered.insert(parent),
            };
            if roots.len() + answered.len() >= query.window {
                window_met = true;
                break;
            }
        }

        if window_met || scanned == SCAN_LIMIT {
            break;
        }
        let Some(next) = page.next_before else {
            break;
        };
        cursor = Some(next);
    }

    candidates.reverse();
    let projected = narrow_to_roots_and_first_replies(candidates, &query.viewer);
    // Every survivor was counted by the estimate above, and the walk stops the
    // moment that estimate reaches the window, so the window needs no second
    // enforcement here — and enforcing it would have to trim the newest end.
    debug_assert!(projected.len() <= query.window);
    Ok(projected)
}

/// Keep every root and each root's first reply, from a chronological slice.
///
/// A reply whose parent is itself a reply is a thread interior and never
/// promoted; a reply whose parent fell outside the scan cannot be shown to be a
/// root and is dropped rather than flattened into the channel.
fn narrow_to_roots_and_first_replies(
    candidates: Vec<Candidate>,
    viewer: &Viewer,
) -> Vec<SessionMessage> {
    let roots: BTreeSet<Sequence> = candidates
        .iter()
        .filter(|candidate| candidate.parent.is_none())
        .map(|candidate| candidate.sequence)
        .collect();
    let mut promoted: BTreeSet<Sequence> = BTreeSet::new();
    let mut projected: Vec<SessionMessage> = Vec::new();

    for candidate in candidates {
        if candidate.content.trim().is_empty() {
            continue;
        }
        let keep = match candidate.parent {
            None => true,
            Some(parent) => roots.contains(&parent) && promoted.insert(parent),
        };
        if keep {
            projected.push(present(
                candidate.sequence,
                candidate.author,
                candidate.content,
                candidate.audience,
                viewer,
            ));
        }
    }

    projected
}

pub(crate) fn validate_page(
    page: &SessionPage,
    before: Option<Sequence>,
    requested: usize,
    seen: &mut Vec<Sequence>,
) -> Result<()> {
    if page.messages.len() > requested {
        return Err(Error::PageTooLarge {
            requested,
            actual: page.messages.len(),
        });
    }
    if page.messages.is_empty() {
        return match page.next_before {
            Some(next_before) => Err(Error::EmptyPageCursor { next_before }),
            None => Ok(()),
        };
    }

    for (index, message) in page.messages.iter().enumerate() {
        if seen.contains(&message.sequence) {
            return Err(Error::DuplicateSequence {
                sequence: message.sequence,
            });
        }
        if let Some(bound) = before
            && message.sequence >= bound
        {
            return Err(Error::PageOutOfRange {
                sequence: message.sequence,
                before: bound,
            });
        }
        if let Some(previous) = index
            .checked_sub(1)
            .map(|prior| page.messages[prior].sequence)
            && previous <= message.sequence
        {
            return Err(Error::PageNotDescending {
                previous,
                next: message.sequence,
            });
        }
        seen.push(message.sequence);
    }

    if let Some(next_before) = page.next_before {
        if let Some(bound) = before
            && next_before >= bound
        {
            return Err(Error::CursorDidNotAdvance {
                before: bound,
                next_before,
            });
        }
        let oldest = page.messages[page.messages.len() - 1].sequence;
        if next_before > oldest {
            return Err(Error::CursorAfterOldest {
                next_before,
                oldest,
            });
        }
    }
    Ok(())
}

pub(crate) fn in_desk(message: &LogMessage, conversation: &Conversation) -> bool {
    same_conversation(message.chat_id.as_deref(), Some(&conversation.desk_id))
        || same_conversation(message.chat_id.as_deref(), Some(&conversation.desk_name))
}

/// Admit a row addressed to exactly this conversation, one row at a time.
///
/// The channel-level arm is deliberately roots-only. Promoting each root's
/// first reply needs the whole slice — see
/// [`narrow_to_roots_and_first_replies`] — so a caller that decides row by row,
/// such as the sharing delta, keeps the narrower rule and delivers a first
/// reply on the next reseed rather than the next tick.
pub(crate) fn matches_conversation(message: &LogMessage, conversation: &Conversation) -> bool {
    if !in_desk(message, conversation) {
        return false;
    }
    match conversation.thread_root {
        None => message.parent.is_none(),
        Some(root) => message.sequence == root || message.parent == Some(root),
    }
}
