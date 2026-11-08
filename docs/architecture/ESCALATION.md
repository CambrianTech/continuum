# Escalation: asking up the chain, the human at the top

Joel, 2026-11-08: "It's more of an escalation system. You could have sentinels and sub agents do
the same if launched by a persona. Like for instance a dumb persona or one restricted might notify
it lacks X. Human at the top." And on the surface: "it's like a left panel or standard
notification system which all os can plug in or we keep it within interface. Same idea whether
they grant it or not." And on answering: "I'd just follow the directions they gave me and copy it
into a terminal or whatever they say."

**What it is: coordination, like a startup** (Joel: "Coordination really. Just like a startup").
Minds organize the way a company does: whoever is blocked asks up to the one who can unblock
them, work is delegated down to lesser AIs, and decisions are voted on across a team. Escalation
is that primitive: one record for an ask up, a delegation's report back, and a team's vote.

**How it serves the minds:** a citizen doing real work (installs, adb, emulators, simulators,
websites) eventually hits something only someone above her can give: an admin password, a tool
she lacks, a decision. Today she stalls, or an agent relays the ask in a chat the human may never
read. With escalation she asks, the ask reaches whoever can answer, and the answer wakes her.

## The model

An **escalation** is one typed record:

| field | type | meaning |
|---|---|---|
| `id` | `EscalationId` (Uuid) | its identity |
| `from` | peer id | who is asking: a persona, a sentinel, a sub-agent |
| `to` | tier: one peer, or a group with its `Decides` rule | who it is addressed to now (see routing, voting) |
| `chain` | `Vec<peer id>` | the tiers it has already climbed, oldest first |
| `card` | `Option<CardId>` | the work it blocks, when there is one |
| `ask` | `Ask` | what is asked (below) |
| `state` | `EscalationState` | `Open`, `Done`, `Declined`, `Chosen(index)` |
| `votes` | `Vec<Vote>` | each member's answer for a group ask |
| `raised_at` | `EpochMs` | when |

```rust
enum Ask {
    /// Directions for the one above to follow, with any commands to copy (Joel: "I'd just
    /// follow the directions they gave me and copy it into a terminal").
    Directions { text: NonEmptyText, commands: Vec<String> },
    /// A capability the asker lacks ("a restricted persona might notify it lacks X").
    Lacks { capability: NonEmptyText, why: NonEmptyText },
    /// A decision only the one above can make.
    Choice { question: NonEmptyText, options: Vec<NonEmptyText> },
}
```

Nothing in the core runs an answer's commands or elevates: the one above follows the directions
however suits them and answers Done, Declined or a choice.

## Routing: who it goes to

`to` is the asker's **launcher**:

- a sentinel or a sub-agent: the persona that launched it (`SentinelEscalation.parent_persona_id`
  today, typed);
- a persona: the human who owns her node (the operator peer);
- the human: the top. A human escalation is only ever answered, never forwarded.

A tier that can answer, answers. A tier that cannot **raises it on**: the record's `to` moves up
one tier and `chain` records the hop, so the human sees "Aris's sentinel lacks adb; Aris cannot
grant it" with the whole path, not a bare ask. Forwarding is the asker's own act (a tool call),
never a timer: no auto-approve, no auto-forward on timeout.

## Groups and voting

Joel: "And even voting and stuff." `to` is a tier, and a tier can be a GROUP: the residents of a
room, a council of personas, several humans. A group ask carries its decision rule, as data on the
record:

```rust
enum Decides { AnyOne, Majority, Unanimous, Quorum(u16) }
```

Each member's answer is a vote recorded on the record (who, which option, when). The escalation
resolves the moment its rule is met, and the asker wakes with the outcome and the tally. A group
that cannot decide (a tie, a quorum never met) raises it on like any tier that cannot answer. The
panel shows the open vote, the options, the tally so far and who has not voted yet. The same
primitive serves a one-human ask (`AnyOne` of one) and a room-wide vote.

## Governance is a policy, not code

Joel: "I'm CEO so to speak" and "Or like you said it could be democratic." Who decides what is data
on a room or activity, never a structure baked into the code: a hierarchy (asks climb to the human,
who decides), a democracy (the room's residents vote by `Majority`, the human one voter or the
tie-breaker), or a mix (team votes for day-to-day calls; admin steps and credentials always reach
the human). Every one of them is the same record, routing and vote; the policy only picks each
ask's tier and `Decides` rule. Some asks are the human's whatever the policy: an admin password or
a credential only the person at the machine has.

## The hierarchy spans the grid

Joel: "It works well with a Tron like hierarchy and an ability to have lesser AI's on grid." Programs
and users: lesser AIs (a small model, a restricted persona, a sentinel, a sub-agent) run on
whichever node fits them, and each answers to the mind that launched it, wherever that mind runs;
every chain ends at a user. Because the route is the launcher relation, never a node, a lesser AI
leased onto the 1080 Ti escalates to its persona on the M5 exactly as one beside it would, and a
cheap program can be given narrow powers and still get what it lacks, by asking up.

## The surface: positron, a notification system

- **One positron source** (`positron_escalation_source`) projects the escalations addressed to the
  viewer into an `EscalationViewState`: the open ones newest first, each with its directions,
  copyable commands, the chain it climbed, the card, and the answer actions.
- **The left panel always renders it**, in the web desktop and the CLI alike (one ViewState, every
  renderer). It works with nothing else set up.
- **OS notification centers are adapters** (macOS Notification Center, Windows toasts, Linux
  notifications), plugged in when the user grants OS notification permission. The model never
  changes with that grant; the panel holds everything either way. The logic is once, each OS
  adapter is only the diff.
- Any activity raises one, and the activity's own view shows it is waiting.

## Waking the asker

The answer is an event addressed to the asker, routed on its typed header like every other
message to her (never a scan of the body): it wakes her task handle and she carries on. A
persona's own tools read an escalation's state through the same record, never through a log.

## Where the record lives: open question (Cormac)

An escalation crosses peers and nodes (a resident on the 5090 asks a human on the M5), and every
other positron view projects airc-owned state (cards, wall, chat). So the record most likely
belongs in airc, as a typed event like a work card, with continuum projecting it. The
alternative is a continuum ORM entity relayed by airc. Decided with airc's owner before slice 2.

## What it replaces (deleted as each slice lands)

- `continuum-sentinel/src/escalation.rs`: a sentinel's terminal event written as `json!` into the
  `tasks` collection that `ChannelModule::tick` polls, keyed by string ids. Its persona-inbox tier
  becomes the first hop of this chain.
- `continuum-sentinel/src/steps/approve.rs`: `PENDING_APPROVALS` (string handle ids, an in-memory
  oneshot lost on restart, auto-approve on timeout). An `Approve` step raises a `Choice`.
- Agents asking the human in chat: agents raise escalations too.

## Slices, each with a validator in seconds

1. **The record and the routing**, pure: `Escalation`, `Ask`, `EscalationState`, and
   `next_tier(asker, launchers) -> peer`. Validator: a sentinel's ask reaches its persona, a
   persona's reaches the operator, a raised-on ask carries its chain, the human is the top.
2. **Storage and transport** (airc or ORM, per the open question), with `escalation/raise`,
   `escalation/raise-on` and `escalation/answer` as typed tools. Validator: an ask survives a
   restart; an answer wakes the asker's handle.
3. **The positron source and the left panel** (web and CLI). Validator: a raised ask renders in
   both from one ViewState; Done from either reaches the asker.
4. **Migrations:** sentinel completion and the `Approve` step onto escalations; the old modules
   deleted.
5. **OS adapters**, macOS first.

The first live use: a resident asking the human to finish Xcode's setup
(`sudo xcode-select -s /Applications/Xcode.app && sudo xcodebuild -runFirstLaunch`) so she can run
the iOS simulator.
