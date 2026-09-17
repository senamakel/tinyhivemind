# Select

Ranking a query against borrowed candidates, once, for every picker. Agents,
desks, and anything else offered by name are all selected the same way: a
short query, a bounded list of candidates, and a deterministic order over the
ones that matched. Writing that ordering once means an agent search and a desk
search cannot disagree about whether a prefix beats a substring, and the
ordering itself is a fold a test can pin exactly. [`find`](../find/README.md)
is the module that actually builds candidates from a roster or desk snapshot
and calls into this one.

Scoring is fixed-point integer arithmetic — a tier base plus a density term —
so every score is reproducible across machines and every payload here derives
`Eq`.

## Files

- `types.rs` — the stable value types: `MatchKind`, `MatchField`, `TextMatch`,
  `Candidate`, `Hit`, and `Pattern`.
- `mod.rs` — `score`, `score_pattern`, `rank`, `rank_pattern`,
  `regex_source`, and the private matching helpers they are built from.
- `test.rs` — unit tests pinning the tier ordering, density tie-breaking, and
  the text/regex pattern parity.

## Public surface

- `SELECT_LIMIT` — the default number of hits a picker offers (8); a longer
  list stops being a choice and becomes a second search.
- `DENSITY_SCALE` — the largest density bonus a match can earn (100,
  fixed-point).
- `MatchKind` — the five match tiers, worst first: `Subsequence`,
  `Substring`, `WordPrefix`, `Prefix`, `Exact`. Tiers are 200 points apart, so
  density (at most 100) can never promote a weaker tier past a stronger one.
- `MatchField` — which piece of a candidate matched: `Detail`, `Id`, `Label`.
- `TextMatch` — one scored match: its `kind`, `score`, and character `offset`.
- `Candidate` — one thing a query may select: `id`, `label`, optional `detail`.
  Borrowed, has no serde representation — a host stores the record it was
  built from, never the candidate.
- `Hit` — one ranked result, borrowed from the snapshot it came from.
- `Pattern` — what to match against: `Text` (case-insensitive literal) or,
  behind the `regex` feature, `Regex` (a borrowed compiled expression). Both
  score onto the same tiers.
- `score` / `score_pattern` — score one query/pattern against one text.
- `rank` / `rank_pattern` — rank a candidate slice, best first, truncated to a
  limit.
- `regex_source` — read the source out of a `/…/`-delimited query, without
  compiling it; compilation (syntax, flags, size limits) is the caller's own
  decision.

## Design notes

**Field precedence.** A candidate is scored on its label and id at full
weight and on its detail at half; ties resolve label, then id, then detail
(`best_field`'s ordering). A detail match is *halved*, not dropped a tier —
strong supporting-text evidence still beats a weak name match, but a
description can never outrank the thing the query actually named.

**Sort order.** `rank_pattern` sorts by `(score desc, offset asc, label length
asc)`, then candidate input order as the final, stable tiebreak — the same
snapshot and query always produce the same list.

**Regex scoring.** A regex match is read onto the same five tiers by its span:
covering the whole text is `Exact`, starting at offset zero is `Prefix`,
starting a word is `WordPrefix`, otherwise `Substring`. A zero-width match
(`^`, a lookaround-only `\b`, an all-optional pattern) is the weakest possible
match: `Substring` with no density credit.

## Operational constraints

- Both text and detail comparisons are performed on trimmed, lowercased
  `char` sequences; a regex pattern is matched against trimmed text *as
  written* — case folding there is the expression's own business (`(?i)`).
- A blank query matches nothing; an empty picker query is a request to list,
  and listing is the caller's own snapshot to iterate, not this module's job.
- `rank`/`rank_pattern` return an empty vector for `limit == 0` without
  scoring anything.
- No IO, no floating point; `ratio` is a saturating fixed-point division.
