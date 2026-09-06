//! Unit tests for roster structure and payload representation.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Person, Roster, RosterMember};
use crate::error::Error;
use serde_json::json;

#[test]
fn pins_roster_payload_wire_shapes() {
    assert_eq!(
        serde_json::to_value(RosterMember {
            id: "alice".into(),
            name: None,
        })
        .unwrap(),
        json!({"id": "alice", "name": null})
    );
    assert_eq!(
        serde_json::to_value(Person {
            id: "p1".into(),
            label: "Ada".into(),
        })
        .unwrap(),
        json!({"id": "p1", "label": "Ada"})
    );
}

#[test]
fn roster_member_name_is_required_but_accepts_null() {
    assert!(serde_json::from_value::<RosterMember>(json!({"id": "alice"})).is_err());
    assert_eq!(
        serde_json::from_value::<RosterMember>(json!({"id": "alice", "name": null})).unwrap(),
        RosterMember {
            id: "alice".into(),
            name: None,
        }
    );
}

#[test]
fn validates_ids_per_namespace_but_allows_alias_collisions() {
    let members = [
        RosterMember {
            id: "same".into(),
            name: Some("Shared".into()),
        },
        RosterMember {
            id: "other".into(),
            name: Some("Shared".into()),
        },
    ];
    let people = [Person {
        id: "same".into(),
        label: "Shared".into(),
    }];
    assert_eq!(Roster::new(&members, &people, &[]).validate(), Ok(()));
}

#[test]
fn rejects_blank_and_duplicate_ids() {
    let blank = [RosterMember {
        id: "  ".into(),
        name: None,
    }];
    assert_eq!(
        Roster::new(&blank, &[], &[]).validate(),
        Err(Error::EmptyRosterMemberId)
    );

    let people = [
        Person {
            id: "p".into(),
            label: "One".into(),
        },
        Person {
            id: "p".into(),
            label: "Two".into(),
        },
    ];
    assert_eq!(
        Roster::new(&[], &people, &[]).validate(),
        Err(Error::DuplicatePersonId {
            person_id: "p".into()
        })
    );

    let members = [member_for_test("a"), member_for_test("a")];
    assert_eq!(
        Roster::new(&members, &[], &[]).validate(),
        Err(Error::DuplicateRosterMemberId {
            member_id: "a".into()
        })
    );

    let blank_person = [Person {
        id: "\t".into(),
        label: String::new(),
    }];
    assert_eq!(
        Roster::new(&[], &blank_person, &[]).validate(),
        Err(Error::EmptyPersonId)
    );
}

fn member_for_test(id: &str) -> RosterMember {
    RosterMember {
        id: id.into(),
        name: None,
    }
}

#[test]
fn active_lookup_excludes_only_exact_retired_ids() {
    let members = [
        RosterMember {
            id: "alice".into(),
            name: None,
        },
        RosterMember {
            id: "Alice".into(),
            name: None,
        },
    ];
    let retired = [String::from("alice")];
    let roster = Roster::new(&members, &[], &retired);
    assert_eq!(
        roster
            .active_members()
            .map(|member| member.id.as_str())
            .collect::<Vec<_>>(),
        vec!["Alice"]
    );
}

#[test]
fn a_member_stays_attributable_after_it_stops_being_active() {
    let members = [
        RosterMember {
            id: "alice".into(),
            name: Some("Alice".into()),
        },
        RosterMember {
            id: "bob".into(),
            name: Some("Bob".into()),
        },
    ];
    let retired = [String::from("bob")];
    let roster = Roster::new(&members, &[], &retired);

    assert!(roster.active_member("bob").is_none());
    assert_eq!(
        roster.registered_member("bob").and_then(|m| m.name.clone()),
        Some("Bob".into())
    );
    assert_eq!(
        roster
            .registered_member("alice")
            .map(|member| member.id.as_str()),
        Some("alice")
    );
    assert!(roster.registered_member("ghost").is_none());
}

#[test]
fn a_tombstoned_member_stays_registered_and_never_runs() {
    let members = [
        RosterMember {
            id: "alice".into(),
            name: Some("Alice".into()),
        },
        RosterMember {
            id: "bob".into(),
            name: Some("Bob".into()),
        },
    ];
    let tombstoned = [String::from("bob")];
    let roster = Roster::new(&members, &[], &[]).with_tombstoned(&tombstoned);

    assert_eq!(
        roster
            .active_members()
            .map(|member| member.id.as_str())
            .collect::<Vec<_>>(),
        vec!["alice"]
    );
    assert!(roster.active_member("bob").is_none());
    assert_eq!(
        roster.registered_member("bob").and_then(|m| m.name.clone()),
        Some("Bob".into())
    );
}

#[test]
fn a_tombstone_holds_even_when_the_host_also_calls_the_member_retired() {
    let members = [RosterMember {
        id: "bob".into(),
        name: None,
    }];
    let bob = [String::from("bob")];
    let roster = Roster::new(&members, &[], &bob).with_tombstoned(&bob);

    assert!(roster.active_member("bob").is_none());
    assert!(roster.registered_member("bob").is_some());
}

#[test]
fn asking_whether_a_member_may_run_cannot_tell_the_unavailable_states_apart() {
    let members = [
        member_for_test("active"),
        member_for_test("retired"),
        member_for_test("tombstoned"),
    ];
    let retired = [String::from("retired")];
    let tombstoned = [String::from("tombstoned")];
    let roster = Roster::new(&members, &[], &retired).with_tombstoned(&tombstoned);

    // An id the roster never held, one it retired, and one it tombstoned all
    // get the same refusal, so no caller can enumerate the roster by asking.
    for id in ["retired", "tombstoned", "never_existed"] {
        assert!(roster.active_member(id).is_none(), "{id} must not run");
    }

    // The retirement predicate does not split the two kept states either.
    assert_eq!(roster.is_retired("retired"), roster.is_retired("tombstoned"));
    assert!(roster.is_retired("tombstoned"));
    assert!(!roster.is_retired("never_existed"));
    assert!(!roster.is_retired("active"));
}
