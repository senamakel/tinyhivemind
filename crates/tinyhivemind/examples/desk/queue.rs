//! The host's turn queue: what is waiting to run, and its idempotency.
//!
//! `tinyhivemind` decides *whether* a committed reply may start a child turn;
//! it holds no queue of its own. [`DeskQueue`] is this host's answer to the
//! [`MentionTurnQueue`] port — a mutex-guarded deque plus a seen-set keyed by
//! `(trigger sequence, target)`, so a duplicate enqueue for the same trigger
//! is refused rather than run twice.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use tinyhivemind::{EnqueueOutcome, MentionTurnFuture, MentionTurnQueue, MentionTurnRequest};

/// One turn waiting to run.
#[derive(Clone, Debug)]
pub(crate) struct PendingTurn {
    /// The seat this turn is addressed to.
    pub(crate) target_id: String,
    /// The message that addressed it, verbatim.
    pub(crate) trigger: String,
    /// How many hand-offs already led to this turn.
    pub(crate) hop: u32,
}

/// The host's atomic enqueue boundary: here, a mutex and a queue.
#[derive(Clone, Default)]
pub(crate) struct DeskQueue {
    /// Turns accepted by `enqueue_once`, waiting for their turn to run.
    pub(crate) pending: Arc<Mutex<VecDeque<PendingTurn>>>,
    /// Every `(trigger sequence, target id)` already enqueued, so a repeat
    /// request for the same pair is answered `Already` rather than queued
    /// again.
    pub(crate) seen: Arc<Mutex<Vec<(u64, String)>>>,
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
