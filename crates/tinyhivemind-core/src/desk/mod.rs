//! Declared desks merged with host-owned membership overlays.

#[cfg(test)]
mod test;

mod types;

pub use types::{Desk, DeskMember, DeskOrder, ResponderMode};

use crate::{
    chat::{GENERAL_DESK, MAIN_THREAD_ID},
    error::{Error, Result},
};

/// A borrowed view over declared desks and host-owned overlay records.
///
/// The view retains no merged state. Each operation deterministically folds
/// the borrowed snapshots in declared-then-added input order.
#[derive(Debug)]
pub struct DeskSet<'a> {
    declared: &'a [Desk],
    added: &'a [Desk],
    member_additions: &'a [DeskMember],
    orders: &'a [DeskOrder],
    retired_agent_ids: &'a [String],
    tombstoned_agent_ids: &'a [String],
}

impl<'a> DeskSet<'a> {
    /// Borrow the snapshots that define the current desk view.
    ///
    /// No agent is tombstoned; add that snapshot with
    /// [`Self::with_tombstoned`].
    #[must_use]
    pub const fn new(
        declared: &'a [Desk],
        added: &'a [Desk],
        member_additions: &'a [DeskMember],
        orders: &'a [DeskOrder],
        retired_agent_ids: &'a [String],
    ) -> Self {
        Self {
            declared,
            added,
            member_additions,
            orders,
            retired_agent_ids,
            tombstoned_agent_ids: &[],
        }
    }

    /// Borrow the ids of agents that were removed for good.
    ///
    /// A tombstoned agent leaves every desk it was declared or added to, so
    /// [`Self::lead`] and [`Self::members`] cannot name one even when the host
    /// has not also retired it. See [`Roster`](crate::roster::Roster) for the
    /// three states and why a tombstone is not merely a retirement.
    #[must_use]
    pub const fn with_tombstoned(mut self, tombstoned_agent_ids: &'a [String]) -> Self {
        self.tombstoned_agent_ids = tombstoned_agent_ids;
        self
    }

    /// Iterate declared then operator-added desk records.
    pub fn iter(&self) -> impl Iterator<Item = &'a Desk> + '_ {
        self.desks()
    }

    /// Resolve an exact desk id or exact display name to its canonical id.
    ///
    /// Exact ids take precedence over names. Named ids and names compare
    /// case-sensitively.
    ///
    /// # Errors
    ///
    /// Returns a validation error if any borrowed record is malformed,
    /// [`Error::UnknownDesk`] if no desk matches, or [`Error::AmbiguousDesk`]
    /// if multiple desks have the requested exact name.
    pub fn resolve_id(&self, identity: &str) -> Result<&'a str> {
        self.validate()?;

        if let Some(desk) = self.desks().find(|desk| desk.id == identity) {
            return Ok(&desk.id);
        }

        let mut matches = self.desks().filter(|desk| desk.name == identity);
        let Some(first) = matches.next() else {
            return Err(Error::UnknownDesk {
                identity: identity.into(),
            });
        };
        if matches.next().is_some() {
            return Err(Error::AmbiguousDesk {
                identity: identity.into(),
            });
        }
        Ok(&first.id)
    }

    /// Report whether an exact id or name resolves unambiguously.
    ///
    /// A malformed borrowed snapshot returns `false`; call [`Self::validate`]
    /// when the particular failure matters.
    #[must_use]
    pub fn contains(&self, identity: &str) -> bool {
        self.resolve_id(identity).is_ok()
    }

    /// Return a desk's deduplicated, available members in effective order.
    ///
    /// Retired and tombstoned agents are excluded, and nothing in the result
    /// says which of the two exclusions applied. A stored order that still
    /// names a now-unavailable agent is not rejected: the stale entry is
    /// skipped rather than forcing the host to rewrite the order the moment
    /// an agent retires or is tombstoned.
    ///
    /// # Errors
    ///
    /// Returns any error documented by [`Self::validate`] or [`Self::resolve_id`].
    pub fn members(&self, identity: &str) -> Result<Vec<&'a str>> {
        let desk_id = self.resolve_id(identity)?;
        if let Some(order) = self.orders.iter().find(|order| order.desk_id == desk_id) {
            return Ok(order
                .ordered
                .iter()
                .map(String::as_str)
                .filter(|agent_id| !self.is_unavailable(agent_id))
                .collect());
        }
        Ok(self.base_members(desk_id))
    }

    /// Return the first effective member of a desk, if it has one.
    ///
    /// # Errors
    ///
    /// Returns any error documented by [`Self::members`].
    pub fn lead(&self, identity: &str) -> Result<Option<&'a str>> {
        Ok(self.members(identity)?.first().copied())
    }

    /// Validate every desk, membership addition, and whole-set member order.
    ///
    /// Checks records in declared-then-added order and overlays in their input
    /// order, returning the first typed failure.
    ///
    /// # Errors
    ///
    /// Returns the specific [`Error`] variant for the first empty, duplicate,
    /// reserved, unknown, or invalidly ordered record.
    pub fn validate(&self) -> Result<()> {
        let mut seen_ids: Vec<&str> = Vec::new();
        for desk in self.desks() {
            if desk.id.is_empty() {
                return Err(Error::EmptyDeskId);
            }
            if desk.name.is_empty() {
                return Err(Error::EmptyDeskName {
                    desk_id: desk.id.clone(),
                });
            }
            if seen_ids.contains(&desk.id.as_str()) {
                return Err(Error::DuplicateDeskId {
                    desk_id: desk.id.clone(),
                });
            }
            seen_ids.push(&desk.id);

            let is_default = desk.id == GENERAL_DESK && desk.name == GENERAL_DESK;
            if !is_default {
                for identity in [&desk.id, &desk.name] {
                    if identity.eq_ignore_ascii_case(GENERAL_DESK)
                        || identity.eq_ignore_ascii_case(MAIN_THREAD_ID)
                    {
                        return Err(Error::ReservedDeskIdentity {
                            identity: identity.clone(),
                        });
                    }
                }
            }
        }

        for addition in self.member_additions {
            if self.find_id(&addition.desk_id).is_none() {
                return Err(Error::UnknownMemberDesk {
                    desk_id: addition.desk_id.clone(),
                });
            }
        }

        let mut ordered_desks: Vec<&str> = Vec::new();
        for order in self.orders {
            if self.find_id(&order.desk_id).is_none() {
                return Err(Error::UnknownOrderDesk {
                    desk_id: order.desk_id.clone(),
                });
            }
            if ordered_desks.contains(&order.desk_id.as_str()) {
                return Err(Error::DuplicateDeskOrder {
                    desk_id: order.desk_id.clone(),
                });
            }
            ordered_desks.push(&order.desk_id);
            self.validate_order(order)?;
        }
        Ok(())
    }

    fn desks(&self) -> impl Iterator<Item = &'a Desk> + '_ {
        self.declared.iter().chain(self.added)
    }

    fn find_id(&self, desk_id: &str) -> Option<&'a Desk> {
        self.desks().find(|desk| desk.id == desk_id)
    }

    fn base_members(&self, desk_id: &str) -> Vec<&'a str> {
        let mut members = Vec::new();
        for member in self.raw_members(desk_id) {
            if !self.is_unavailable(member) && !members.contains(&member) {
                members.push(member);
            }
        }
        members
    }

    /// Every declared or added member of `desk_id`, before filtering out
    /// unavailable agents.
    ///
    /// Used to tell "this agent belonged to the desk and later became
    /// unavailable" from "this agent never belonged to the desk at all" —
    /// [`Self::is_unavailable`] alone cannot distinguish the two, and a
    /// stored order naming an id from the second group is malformed, not
    /// merely stale.
    fn raw_members(&self, desk_id: &str) -> impl Iterator<Item = &'a str> + '_ {
        let declared = self
            .find_id(desk_id)
            .into_iter()
            .flat_map(|desk| desk.members.iter().map(String::as_str));
        let added = self
            .member_additions
            .iter()
            .filter(move |addition| addition.desk_id == desk_id)
            .map(|addition| addition.agent_id.as_str());
        declared.chain(added)
    }

    fn is_unavailable(&self, agent_id: &str) -> bool {
        self.retired_agent_ids
            .iter()
            .chain(self.tombstoned_agent_ids)
            .any(|unavailable| unavailable == agent_id)
    }

    fn validate_order(&self, order: &DeskOrder) -> Result<()> {
        let members = self.base_members(&order.desk_id);
        let raw_members: Vec<&str> = self.raw_members(&order.desk_id).collect();
        let mut seen: Vec<&str> = Vec::new();
        for agent_id in &order.ordered {
            // A now-unavailable agent that *was* a member of this desk may
            // still be named by a stored order: the host is not required to
            // rewrite the order the moment an agent retires or is
            // tombstoned, so a stale entry is skipped rather than treated as
            // an unknown or duplicate member. An unavailable id that was
            // never a member of this desk at all is a different, genuine
            // error — global unavailability elsewhere must not launder a
            // malformed order past validation.
            if self.is_unavailable(agent_id) && raw_members.contains(&agent_id.as_str()) {
                continue;
            }
            if seen.contains(&agent_id.as_str()) {
                return Err(Error::DuplicateOrderMember {
                    desk_id: order.desk_id.clone(),
                    agent_id: agent_id.clone(),
                });
            }
            seen.push(agent_id);
            if !members.contains(&agent_id.as_str()) {
                return Err(Error::UnknownOrderMember {
                    desk_id: order.desk_id.clone(),
                    agent_id: agent_id.clone(),
                });
            }
        }
        if let Some(missing) = members.iter().find(|member| !seen.contains(member)) {
            return Err(Error::IncompleteOrder {
                desk_id: order.desk_id.clone(),
                missing_agent_id: (*missing).into(),
            });
        }
        Ok(())
    }
}
