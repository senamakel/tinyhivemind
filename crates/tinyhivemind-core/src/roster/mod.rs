//! Agent and person identities visible in one shared conversation.

#[cfg(test)]
mod test;

mod types;

pub use types::{Person, RosterMember};

use crate::error::{Error, Result};

/// A borrowed view of current agent and person identities.
///
/// An agent is in one of three states, and the state is a fold over the
/// borrowed id lists rather than a flag on the record:
///
/// - **Active** — in `members`, in neither id list. It may run, it is
///   addressable, and it is part of `@everyone`.
/// - **Retired** — in `retired_member_ids`. Out of rotation, and the host may
///   put it back by dropping the id from that list.
/// - **Tombstoned** — in `tombstoned_member_ids`. Removed for good. It is
///   refused exactly as a retired agent is, but the host keeps its
///   [`RosterMember`] registered so messages it already committed still render
///   with their author's name. Because the record stays, the id stays taken:
///   re-registering it is a [`Error::DuplicateRosterMemberId`] rather than a
///   silent re-attribution of somebody else's history.
#[derive(Debug)]
pub struct Roster<'a> {
    members: &'a [RosterMember],
    people: &'a [Person],
    retired_member_ids: &'a [String],
    tombstoned_member_ids: &'a [String],
}

impl<'a> Roster<'a> {
    /// Borrow the snapshots that define the current roster.
    ///
    /// No agent is tombstoned; add that snapshot with
    /// [`Self::with_tombstoned`].
    #[must_use]
    pub const fn new(
        members: &'a [RosterMember],
        people: &'a [Person],
        retired_member_ids: &'a [String],
    ) -> Self {
        Self {
            members,
            people,
            retired_member_ids,
            tombstoned_member_ids: &[],
        }
    }

    /// Borrow the ids of agents that were removed for good.
    ///
    /// A tombstoned agent must stay in the `members` snapshot: that is what
    /// keeps its old messages attributable. Everything else refuses it.
    #[must_use]
    pub const fn with_tombstoned(mut self, tombstoned_member_ids: &'a [String]) -> Self {
        self.tombstoned_member_ids = tombstoned_member_ids;
        self
    }

    /// Validate structural identity invariants.
    ///
    /// Agent and person ids occupy independent namespaces. Display aliases are
    /// deliberately allowed to collide and fail closed during mention lookup.
    ///
    /// # Errors
    ///
    /// Returns a typed error for the first blank or duplicate id.
    pub fn validate(&self) -> Result<()> {
        let mut member_ids = Vec::new();
        for member in self.members {
            if member.id.trim().is_empty() {
                return Err(Error::EmptyRosterMemberId);
            }
            if member_ids.contains(&member.id.as_str()) {
                return Err(Error::DuplicateRosterMemberId {
                    member_id: member.id.clone(),
                });
            }
            member_ids.push(member.id.as_str());
        }

        let mut person_ids = Vec::new();
        for person in self.people {
            if person.id.trim().is_empty() {
                return Err(Error::EmptyPersonId);
            }
            if person_ids.contains(&person.id.as_str()) {
                return Err(Error::DuplicatePersonId {
                    person_id: person.id.clone(),
                });
            }
            person_ids.push(person.id.as_str());
        }
        Ok(())
    }

    /// Iterate active agents in roster order.
    pub fn active_members(&self) -> impl Iterator<Item = &'a RosterMember> + '_ {
        self.members
            .iter()
            .filter(|member| !self.is_retired(&member.id))
    }

    /// Find an active agent by its exact id.
    #[must_use]
    pub fn active_member(&self, id: &str) -> Option<&'a RosterMember> {
        self.active_members().find(|member| member.id == id)
    }

    /// Find any registered agent by its exact id, active or not.
    ///
    /// This is the **attribution** lookup, and it is the only one that sees an
    /// agent the roster no longer runs. A message committed by an agent that
    /// has since been retired still has to render with its author's name, so
    /// the record survives the agent's removal from every active answer.
    ///
    /// It is deliberately not a routing input. Ask [`Self::active_member`]
    /// whether an agent may run; that question has one answer for an unknown id
    /// and for a registered-but-unavailable one, and this one does not.
    #[must_use]
    pub fn registered_member(&self, id: &str) -> Option<&'a RosterMember> {
        self.members.iter().find(|member| member.id == id)
    }

    /// Find a person by its exact id.
    #[must_use]
    pub fn person(&self, id: &str) -> Option<&'a Person> {
        self.people.iter().find(|person| person.id == id)
    }

    /// Iterate people in roster order.
    pub fn people(&self) -> impl Iterator<Item = &'a Person> + '_ {
        self.people.iter()
    }

    /// Report whether an agent id is out of the active roster.
    ///
    /// Retired and tombstoned ids both answer `true`, deliberately: the two
    /// states differ in whether the host may reverse them, never in whether the
    /// agent may run, and a predicate that told them apart would be one more
    /// way to read a roster by asking about names.
    #[must_use]
    pub fn is_retired(&self, id: &str) -> bool {
        self.retired_member_ids.iter().any(|retired| retired == id)
            || self.is_tombstoned_id(id)
    }

    fn is_tombstoned_id(&self, id: &str) -> bool {
        self.tombstoned_member_ids
            .iter()
            .any(|tombstoned| tombstoned == id)
    }
}
