//! Everything one desk run holds: its seats and the storage they read
//! through, bundled so the chain can pass all of it on every turn at once.

use std::sync::Arc;

use crate::agent::Seat;
use crate::host::{Journal, Queue};

/// Everything one desk run holds: its seats and the storage they read
/// through.
///
/// Bundled rather than passed one by one because the chain needs all of it on
/// every turn, and a signature that lists six borrows says less about the
/// shape of a turn than one that says "the room".
pub(crate) struct Room<'a> {
    /// One seat per desk member, in seating order.
    pub(crate) seats: Vec<Seat>,
    /// The same ids, borrowed for the roster and desk views.
    pub(crate) ids: Vec<&'a str>,
    /// The host's journal, shared with the queue that revalidates against it.
    pub(crate) journal: Arc<Journal>,
    /// The host's enqueue boundary over that journal.
    pub(crate) queue: Queue,
    /// The borrowed roster view.
    pub(crate) roster: tinyhivemind_core::roster::Roster<'a>,
    /// The borrowed desk view.
    pub(crate) desks: tinyhivemind_core::desk::DeskSet<'a>,
}

impl std::fmt::Debug for Room<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Room")
            .field("ids", &self.ids)
            .finish()
    }
}
