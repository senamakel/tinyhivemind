# The Grok Bot survey

Twelve notes on the open-source Grok Bot ecosystem, read in September 2026
against the question this workspace asks: *how does a room of agents share one
transcript, and what makes a message start a turn?*

Every note was written from a shallow clone at a pinned commit, reading source
rather than READMEs, and every note closes with the same table — **mechanism →
what tinyhivemind already has → what it does not**. Where a project's README
overclaims relative to its code, the note says so; that happened in eight of
the twelve.

These files hold only the reading. What we decided to do about it belongs in
[`../../specs/`](../../specs/README.md) and [`../../adr/`](../../adr).

## Licence note

[`reconstructed-018.md`](reconstructed-018.md) covers a repository that carries
**no licence** and whose `NOTICE.md` disclaims any upstream grant. It was read
to learn from and nothing in it may be copied. Every other project here is
MIT, Apache-2.0, CC0, CC BY-NC-SA, FSL-1.1-MIT or a Sustainable Use Licence;
each note records which.

## The notes

Closest to what this workspace does, in descending order:

- [`copilotkit-openbot.md`](copilotkit-openbot.md) — CopilotKit/OpenBot. The
  sharpest thinking on delegation: decide/deliver split, a fan-out cap counted
  inside the queue insert, a signed depth assertion, refusals written as
  sentences a model can say. Most design notes name the production bug that
  produced them.
- [`rakazo.md`](rakazo.md) — elie222/rakazo. 224k lines whose layering mirrors
  ours almost exactly, and the best source on recall, model selection as a
  fold, and multi-client sync. Also fans `@everyone` out to N runs.
- [`reconstructed-018.md`](reconstructed-018.md) — b-nnett's reconstruction of
  the shipped 0.18 desktop app. The only look at what the product itself does:
  a bounded round-robin instead of either fan-out or one turn, a `"(pass)"`
  token, and agent-to-agent messaging with no hop bound at all.
- [`openmausbot.md`](openmausbot.md) — milind-soni/OpenMausBot. The closest
  analogue to our roster and mention model, and a live specimen of the
  first-person-collapsed transcript this workspace exists to fix.
- [`orange-book.md`](orange-book.md) — KinGao294's handbook, digested into
  thirteen named patterns, each tagged mechanism or operational advice.
- [`catalogs-and-bloks.md`](catalogs-and-bloks.md) — patterns mined from ~580
  catalogued deployments, a shortlist of further repositories, and
  hamedgitty/bloks, whose seven server files are the densest small example
  here.

Routing, harness boundaries and approval gates:

- [`devspace.md`](devspace.md) — Waishnav/devspace. `selectAcpPermissionOption`
  is the cleanest statement of approval as a pure predicate.
- [`gawkbot.md`](gawkbot.md) — najmuzzaman-mohammad/gawkbot. A real approval
  algebra with standing grants, and a durable blueprint IR.
- [`opengrok.md`](opengrok.md) — OnlyTerp/opengrok. A pure provider ladder with
  the same first-match-wins shape as our responder ladder.

Applications, thin on mechanism:

- [`grok-ship.md`](grok-ship.md) — kunchenguid/grok-ship.
- [`hypergrok-trading-desk.md`](hypergrok-trading-desk.md) — galleonlabs.
- [`gojiberryai-sales-os.md`](gojiberryai-sales-os.md) — romangojiberryAI.

## What the survey found

**Nobody else holds the line on fan-out.** Rakazo expands `@everyone` to one
run per member, Bloks broadcasts a bare message to a room, OpenMausBot starts N
sequential turns, and the shipped product runs a three-round robin. Each then
bolts a bound on afterwards — `GROUP_MEMBER_MAX = 6`, `MAX_AGENT_HOPS`, an
in-process promise chain, three rounds. OpenBot is the one project that reaches
our invariant, and it reaches it independently, by deleting every `@` chip but
the last. That is the strongest external evidence in the survey: four
independent teams shipped the failure mode and then paid for a bound.

**Everybody has an approval gate, and in every case the decision is pure.**
Six projects gate side-effecting actions, and each one separates a pure
`(action, policy) -> allow | deny | ask` decision from the IO that enacts it —
exactly the core/port line this workspace draws. We have no approval concept at
all. It is the largest well-evidenced gap the survey found.

**The transcript is where the bodies are.** OpenMausBot bakes attribution into
prose the model must reparse. OpenBot needed a repair fold after dangling tool
calls permanently poisoned production threads. Rakazo refuses a compaction
summary unless the tail after its cursor is contiguous. A stored transcript is
not automatically a valid prompt, and three teams learned it the same way.
