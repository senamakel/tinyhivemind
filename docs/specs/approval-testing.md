# Approval: test list

Companion to [`approval.md`](approval.md#testing); split out to keep that spec
under this repository's per-Markdown-file line budget. One test per bullet
unless noted.

## Failure paths

- `denies_when_approval_is_disabled`
- `denies_an_empty_actor_id`, `denies_an_empty_call_id`,
  `denies_an_empty_verb`, `denies_an_empty_target`
- `denies_a_target_path_containing_a_parent_component`
- `denies_an_unknown_actor` and `denies_a_retired_actor`
- `denies_an_unclassified_effect`
- `denies_when_one_rule_denies_and_another_allows`
- `denies_when_the_approver_person_is_unknown`
- `denies_when_the_approver_desk_is_unknown`
- `denies_when_the_approver_desk_is_ambiguous`
- `denies_when_no_rule_matches_and_the_default_is_deny`

## Grant liveness and coverage

- `ignores_a_revoked_grant`, `ignores_an_expired_grant`
- `ignores_a_grant_starting_after_now`
- `ignores_a_grant_whose_ttl_exceeds_the_policy_cap`
- `ignores_a_perpetual_grant_when_the_policy_caps_lifetime`
- `ignores_every_grant_when_allow_grants_is_false`
- `a_call_scoped_grant_does_not_cover_another_call`
- `an_action_scoped_grant_covers_another_call`
- `a_resource_grant_does_not_cover_a_sibling_path`
- `a_resource_grant_does_not_cover_a_different_actor`
- `grant_coverage_is_order_independent_and_duplicate_insensitive`

## Epoch-scoped consent, the correctness property

- `denies_a_grant_minted_in_a_later_epoch`
- `denies_a_grant_minted_later_within_the_same_epoch`
- `allows_a_grant_minted_in_an_earlier_epoch`
- `a_refusal_stops_applying_after_the_epoch_advances`
- `a_grant_keeps_applying_after_the_epoch_advances`
- `a_refusal_beats_a_covering_grant_in_the_same_epoch`

## Shape

- `ask_names_a_person_and_never_an_agent`
- `ask_carries_no_wider_scope_than_the_request`
- `minting_the_asked_scope_allows_that_request_and_no_wider_one`
- `approval_never_produces_a_mention_turn_request`

## Approver resolution (`NoApprover` vs. `UnresolvableApprover`)

- `denies_unresolvable_approver_when_the_perdesk_desk_id_is_unknown`
- `denies_unresolvable_approver_when_the_perdesk_desk_id_is_ambiguous`
- `denies_no_approver_when_the_perdesk_resolved_id_names_nobody`
- `denies_no_approver_when_a_bare_person_rule_names_nobody`

## Deterministic grant basis

- `allow_basis_prefers_the_narrowest_scope_when_multiple_grants_cover_a_request`
- `allow_basis_breaks_a_same_scope_tie_on_the_earliest_epoch_and_sequence`
- `allow_basis_is_unaffected_by_permuting_or_duplicating_grants`

## `ScopeKey::render()` collision resistance

- `render_does_not_collide_when_a_field_contains_an_embedded_nul_byte`
- `render_distinguishes_a_named_target_from_a_resource_target_sharing_a_string`

## Remembered refusals as an `approve` input

- `denies_remembered_refusal_when_a_same_epoch_refusal_covers_the_request`
- `ignores_a_refusal_from_a_retired_epoch`
