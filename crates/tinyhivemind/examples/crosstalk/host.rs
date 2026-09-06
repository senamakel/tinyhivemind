//! The host side of one desk: an in-memory journal, its queue, and its roster.
//!
//! Everything here is what a consumer owns rather than what the library
//! provides. `tinyhivemind` opens no database and no socket, so a harness that
//! wants to watch the library route a turn has to supply the storage the
//! library reads through — one [`SessionLog`] over a `Vec` of rows, and one
//! [`MentionTurnQueue`] that records the single child turn a committed reply
//! is allowed to start.
//!
//! The queue is deliberately written the way the port's documentation demands
//! rather than the way a toy would: it revalidates the trigger row against the
//! journal, re-checks the live policy and the target, and refuses a duplicate
//! by key. Those are the checks a real host performs inside a transaction, and
//! writing them out is the point — a harness that skipped them would prove the
//! library dispatches, not that a host can hold the contract.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, PoisonError};

use tinyhivemind::dispatch::{
    EnqueueOutcome, EnqueueRefusal, MentionTurnFuture, MentionTurnQueue, MentionTurnRequest,
};
use tinyhivemind::{
    BoxError, Conversation, LogMessage, Sequence, SessionAuthor, SessionFuture, SessionLog,
    SessionPage,
};
use tinyhivemind_core::aside::Audience;
use tinyhivemind_core::desk::{Desk, DeskSet, ResponderMode};
use tinyhivemind_core::roster::{Person, RosterMember};

/// The canonical desk this harness runs on.
pub(crate) const DESK_ID: &str = "engineering";
/// Its operator-facing display name.
pub(crate) const DESK_NAME: &str = "Engineering";
/// The person who posts the opening instruction.
pub(crate) const OPERATOR_ID: &str = "ada";

/// One append-only journal, held in memory.
///
/// Rows are stored oldest-first and read newest-first, which is the direction
/// [`SessionLog`] specifies. `before` is exclusive.
#[derive(Debug, Default)]
pub(crate) struct Journal {
    rows: Mutex<Vec<LogMessage>>,
}

impl Journal {
    /// Append one row and return the sequence it was given.
    pub(crate) fn append(
        &self,
        conversation: &Conversation,
        author: SessionAuthor,
        content: &str,
    ) -> Sequence {
        let mut rows = self.rows.lock().unwrap_or_else(PoisonError::into_inner);
        let sequence = Sequence(
            u64::try_from(rows.len())
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        );
        rows.push(LogMessage {
            sequence,
            chat_id: Some(conversation.desk_id.clone()),
            parent: conversation.thread_root,
            author,
            content: content.to_owned(),
            audience: Audience::Desk,
        });
        sequence
    }

    /// Return one row by sequence, as a host's transaction would re-read it.
    pub(crate) fn row(&self, sequence: Sequence) -> Option<LogMessage> {
        let rows = self.rows.lock().unwrap_or_else(PoisonError::into_inner);
        rows.iter().find(|row| row.sequence == sequence).cloned()
    }
}

impl SessionLog for Journal {
    fn read_before(&self, before: Option<Sequence>, limit: usize) -> SessionFuture<'_> {
        let rows = self.rows.lock().unwrap_or_else(PoisonError::into_inner);
        let mut page: Vec<LogMessage> = rows
            .iter()
            .rev()
            .filter(|row| before.is_none_or(|bound| row.sequence < bound))
            .take(limit)
            .cloned()
            .collect();
        page.sort_by_key(|row| std::cmp::Reverse(row.sequence));
        let next_before = page.last().map(|row| row.sequence);
        let has_older =
            next_before.is_some_and(|oldest| rows.iter().any(|row| row.sequence < oldest));
        let page = SessionPage {
            messages: page,
            next_before: if has_older { next_before } else { None },
        };
        Box::pin(async move { Ok::<_, BoxError>(page) })
    }
}

/// What the queue did with one request, kept so the harness can print it.
#[derive(Clone, Debug)]
pub(crate) struct Enqueued {
    /// The request the library handed the host, unchanged.
    pub(crate) request: MentionTurnRequest,
}

/// The host's atomic enqueue boundary.
///
/// One `Mutex` stands in for the transaction: the whole revalidation and the
/// write happen under it, so a duplicated trigger cannot race past the
/// idempotency check.
pub(crate) struct Queue {
    journal: Arc<Journal>,
    state: Mutex<QueueState>,
    /// Whether the host still enables the feature. Checked again here, on
    /// purpose: the library's policy check happened before the request was
    /// built, and the port requires the host to re-check under its own lock.
    enabled: bool,
    /// Agent ids the host is willing to run a turn for.
    available: Vec<String>,
}

#[derive(Debug, Default)]
struct QueueState {
    seen: HashSet<(String, Option<u64>, u64)>,
    pending: Vec<Enqueued>,
}

impl Queue {
    /// Build a queue over one journal.
    pub(crate) fn new(journal: Arc<Journal>, enabled: bool, available: &[&str]) -> Self {
        Self {
            journal,
            state: Mutex::new(QueueState::default()),
            enabled,
            available: available.iter().map(|id| (*id).to_owned()).collect(),
        }
    }

    /// Take everything enqueued since the last drain, in arrival order.
    pub(crate) fn drain(&self) -> Vec<Enqueued> {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut state.pending)
    }
}

impl MentionTurnQueue for Queue {
    fn enqueue_once(&self, request: MentionTurnRequest) -> MentionTurnFuture<'_> {
        let outcome = self.transact(&request);
        Box::pin(async move { Ok::<_, BoxError>(outcome) })
    }
}

impl Queue {
    /// The whole transaction, run under one lock.
    fn transact(&self, request: &MentionTurnRequest) -> EnqueueOutcome {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);

        let key = (
            request.conversation.desk_id.clone(),
            request.conversation.thread_root,
            request.key.trigger_sequence,
        );
        if state.seen.contains(&key) {
            return EnqueueOutcome::Already;
        }

        if !self.enabled {
            return EnqueueOutcome::Refused {
                reason: EnqueueRefusal::FeatureDisabled,
            };
        }

        // Re-read the committed reply and verify the library was passed the
        // truth about it. A host that trusts the request here is trusting
        // whatever built it.
        let trigger = self.journal.row(Sequence(request.key.trigger_sequence));
        let authored_by_source = trigger.as_ref().is_some_and(|row| {
            matches!(&row.author, SessionAuthor::Agent { id, .. } if *id == request.source_id)
                && row.content == request.content
                && row.chat_id.as_deref() == Some(request.conversation.desk_id.as_str())
        });
        if !authored_by_source {
            return EnqueueOutcome::Refused {
                reason: EnqueueRefusal::Unauthorized,
            };
        }

        if !self.available.contains(&request.target_id) {
            return EnqueueOutcome::Refused {
                reason: EnqueueRefusal::TargetUnavailable,
            };
        }

        state.seen.insert(key);
        state.pending.push(Enqueued {
            request: request.clone(),
        });
        EnqueueOutcome::Enqueued
    }
}

/// The identities this harness runs with, owned so the borrowed views can
/// point at them for the length of a run.
pub(crate) struct Cast {
    /// Agent roster members, in seating order.
    pub(crate) members: Vec<RosterMember>,
    /// The one human in the room.
    pub(crate) people: Vec<Person>,
    /// Nobody is retired here, but the view still needs the slice.
    pub(crate) retired: Vec<String>,
    /// The one desk.
    pub(crate) desks: Vec<Desk>,
    /// Empty desk overlays.
    pub(crate) added: Vec<Desk>,
}

impl Cast {
    /// Seat `ids` on the desk, with `Auto` responder mode so the ladder's
    /// model-backed rung is reachable.
    pub(crate) fn new(ids: &[&str]) -> Self {
        Self {
            members: ids
                .iter()
                .map(|id| RosterMember {
                    id: (*id).to_owned(),
                    name: None,
                })
                .collect(),
            people: vec![Person {
                id: OPERATOR_ID.to_owned(),
                label: "Ada".to_owned(),
            }],
            retired: Vec::new(),
            desks: vec![Desk {
                id: DESK_ID.to_owned(),
                name: DESK_NAME.to_owned(),
                description: Some("The desk this harness runs on".to_owned()),
                members: ids.iter().map(|id| (*id).to_owned()).collect(),
                responder_mode: ResponderMode::Auto,
            }],
            added: Vec::new(),
        }
    }

    /// Borrow the roster view.
    pub(crate) fn roster(&self) -> tinyhivemind_core::roster::Roster<'_> {
        tinyhivemind_core::roster::Roster::new(&self.members, &self.people, &self.retired)
    }

    /// Borrow the desk view.
    pub(crate) fn desk_set(&self) -> DeskSet<'_> {
        DeskSet::new(&self.desks, &self.added, &[], &[], &self.retired)
    }
}
