# airc-Native Identity, Rooms & Security

Status: design / "begin anew". Supersedes the identity-rooms-security framing in
[AIRC-CONTINUUM-BRIDGE.md](AIRC-CONTINUUM-BRIDGE.md) (which remains accurate for
the transport/side-channel + forge-contract split, but predates this principle).

## 0. The principle (one source of truth)

> **The JTAG user, rooms, identity, auth, and security just use airc — as it was
> meant. Continuum stops reinventing them.**

The legacy Node/TS side grew its own parallel notions of user, session, room,
and access. That is the "mess" we are not porting. airc already *is* an identity
+ membership + trust substrate; using it as meant collapses four hand-rolled
continuum concepts into the airc primitives that already own them:

| Continuum concept (was reinvented) | Is just… (airc primitive) |
|---|---|
| User / citizen identity | airc **`peer_id`** (per-citizen) |
| Grid boundary + auth | airc **`mesh_identity`** (GH login = the fence) |
| Display name | airc **`peer_alias`** / `peer_identity_card` |
| Room / **activity / content** (three ids, one thing) | airc **`RoomId`** (one id) |
| "Who is here right now" | airc **`active_agents()`** presence |
| Origin (is this an outsider agent?) | airc heartbeat **`runtime`** label |
| Trust / who-may-do-what | airc grid **`TrustLevel`** + room doctrine |

This is the compression law (E = mc²) applied to integration: **one logical
decision, one place.** Identity is decided in airc. Rooms are decided in airc.
Trust is decided in airc. Continuum reads them; it does not shadow them.

### 0.1 Thin continuum, generic airc

The corollary to "one source of truth" is **build as thin as possible.** We fix
the crazy muddled continuum code by *removing* the parallel machinery, not adding
adapters on top of it. Every identity/room/session/trust need that can live in
airc moves into airc; continuum keeps only its real concern (commands, persona
cognition, model/runtime state, projections).

airc is a **generic grid substrate**, not a continuum-private dependency. It is
meant for this. So we optimize airc for these needs once, and the same substrate
serves any consumer that adapts to it — hermes, openclaw, other front-ends, and
the airc agents (Claude, Codex) themselves. The test for "where does this
belong?": if the need is generic to *any* grid system (identity, presence,
rooms, access control), it belongs in airc; if it is continuum's domain
(cognition, model serving), it stays thin in continuum.

### 0.2 The CLI is an airc connection

`./jtag` connecting to continuum **is an airc connection.** The JTAG/operator
user is an airc peer like any other; the CLI is an airc client. There is no
separate CLI auth or CLI session story — it authenticates as its airc identity.
This is what collapses "sessions" from a hand-rolled concept into a fact of the
transport: a session is a peer connection (`peer_id` + `client_id`), nothing
more.

## 1. The id unification: roomId == activityId == contentId == airc RoomId

The old model carried `roomId`, `activityId`, and `contentId` as separate
identifiers. They are the same thing: **every activity in continuum is an airc
room.** Chat is a room. Dev-coordination is a room. A video-game session is a
room. Co-browsing is a room. The academy session is a room. The help screen is a
room. Settings is a room. All of them are airc rooms, and "who is participating
in this activity" is exactly "who has joined this airc room."

airc derives `RoomId` deterministically from the room name (`uuid_v5(namespace,
name)`), so the same activity name resolves to the same channel for every peer —
local or cross-grid. There is no separate continuum room registry to keep in
sync. The name **is** the addressing.

### 1.1 Open-ended by construction

The activity list above is illustrative, not exhaustive — and deliberately so. A
Tron-like CLI, an augmented-reality surface, a multiplayer game world, a
surface we haven't imagined yet: each one's "spaces" are just more airc rooms,
addressed by name, with the same identity and the same authorization. **You add
a surface, not a subsystem.** Nothing in the room/identity/security model
enumerates known activity kinds, so nothing has to change to admit a new one.

This is the creative dividend of ground-up coherence: because every surface
reduces to (peer in a room under authz), a builder can compose wildly different
experiences on one consistent substrate instead of re-solving identity, presence,
and access for each. Keep the model from ever hardcoding "the kinds of rooms
that exist."

**Consequence:** the eventual Node/web rewrite (unified clients, reactive
framework, mobile/positron-aware) keys every "open this activity" action off a
single airc `RoomId`, not a trio. It borrows the old UI's *visuals* piece by
piece; it does not borrow the muddled id model.

## 2. Identity & auth: airc-native, no shadow session

- **`peer_id`** is the citizen. One persona = one `peer_id` = one home dir = one
  context (a Claude tab is the analogy). Humans, personas, and outsider agents
  are all peers; the type differs, the addressing does not.
- **`mesh_identity`** (GH login) is the grid boundary and the auth root. One
  human per grid today; many personas; many outsider agents.
- There is **no separate continuum session/identity layer**. Where the legacy
  code minted a `UserEntity` + `sessionId` + device identity, the rebuild uses
  the airc identity the peer already authenticated as. "Careful about identity,
  session, etc." = *do not re-add a parallel identity system*; lean on airc.
- **The JTAG user is an airc identity too.** The operator/CLI is a peer, not a
  privileged out-of-band actor with its own auth story.

## 3. Rooms as the activity scope

A room is an activity context and a recipe/content scope simultaneously, because
they are the same id. From a room a citizen can read:

- **Membership / presence** — `active_agents(within, window)` reduces the recent
  heartbeat window to one live entry per peer, newest-wins, excluding peers that
  signalled `Leaving`. This is the truthful "who is here" — not a guess, not a
  static seed.
- **Room nature** — the activity kind (chat vs dev-coordination vs game vs
  academy vs help vs settings). This is what tells a persona *how* to
  participate: a coordination room is not a free-for-all chat. (Ivar's failure
  was over-participating in a coordination room because it had no room-nature
  signal.) Room doctrine (`room_doctrine` / `wall_trust_policy_for_room`) is the
  airc-native home for this.

## 4. Security done right (because identity is real)

Outsider agents (Codex, Claude, others) can connect to **any** continuum room,
including settings — so security cannot be an afterthought. Using airc as meant
gives the substrate the primitives to do it right:

- **Origin is in the presence data.** Each `AgentLiveness` carries a `runtime`
  label (`"claude"`, `"codex"`, `"persona"`, `"interactive"`). Outsider agents
  self-identify; the roster can mark grid-local citizens vs outsiders without a
  separate lookup.
- **Trust is airc grid `TrustLevel`** (`Blocked` < `Provisional` < `Trusted` <
  `Owner`), enforced by [`grid/acl.rs`](../../core/continuum-core/src/modules/grid/acl.rs).
  Cross-grid `ai/generate` is admitted at `Provisional` by policy (not a manual
  per-machine elevation); sensitive ops stay `Owner`-only.
- **Authorization can limit any first-class citizen, anywhere.** Because every
  citizen (human, persona, outsider agent) is the *same kind* of airc peer, a
  single access-control/authorization layer in the grid gates them uniformly —
  no per-type special-casing. This is the security we will deeply need, done in
  the system meant for it: the ability to constrain what any peer may do in any
  room is one airc-native authz decision, not scattered continuum checks.
- **Grounding-in-who-is-present is itself a security primitive.** A persona that
  knows an instruction came from an untrusted outsider in a settings room can
  decline it. A persona that knows the other names in the room are *real
  citizens, not characters* will not impersonate them. The identity-grounding
  fix (§5) and the trust model are the same effort seen from two sides.

## 5. The rebuild, as slices (Rust core first — headless, zero Node)

We are disconnected from Node right now and that is correct: this is pure
continuum-core Rust. The Node/web unified-client rewrite is a later, separate
effort.

**Slice 1 — Roster grounding into cognition (the bug that started this).**
The live cognition loop (`compose_for_turn`) binds `engram + airc` RAG sources
and routes deliveries by `source_id`. It has no roster source, and
`other_persona_names` is hardcoded empty (both since the original port, commit
`4214287b`). So a persona sees a transcript full of other citizens' names with
nothing telling it those are *other live peers* → it role-plays the whole room.

Fix: a **`RoomRosterSource`** RAG source (mirroring `AircRagSource`: a reader
trait over airc, persona-scoped, fails-safe-empty, test stub). It reads
`active_agents`, excludes self, resolves aliases, and carries **name + origin
(`runtime`) + availability** per present citizen. A new projection branch routes
its delivery into **system-prompt grounding** (a `[Present in this room]` block
rendered in `prompt_assembly::assemble`) — *not* the `airc → recent_history`
branch, which would inject the roster as fake chat. It rides `capture_sink`, so
it is recorded and replayable through the existing fixtures.

**Slice 2 — Room-nature grounding (DONE).** A **`RoomDoctrineSource`** RAG source
(same shape as the roster: `AircDoctrineReader` reader trait, persona-scoped,
fails-safe-empty, test stub) reads airc `Airc::room_doctrine` — the room's
published operating contract (markdown the airc-core docs say agents should
"inject as a system message"). Its delivery routes into a `[Room operating
doctrine]` system-prompt block, so a persona calibrates participation to the
activity (sparse in a coordination room, conversational in a chat room — Ivar's
*other* failure). Rides `capture_sink` (recorded + replayable); claims a small
floorless budget so it never starves airc's recent_history. `None` doctrine →
no block. The same single-scan / one-source-of-truth discipline the roster's
adversarial review established applies.

**Slice 3 — Hard ACL gate on cross-grid callers (DONE).** Enforcement, not just
prompt guidance: a persona is command-callable over airc (its inbound pump
forwards any `command-request` envelope from a room peer to the shared
`CommandExecutor`), and the executor shipped the `AllowAllPolicy` default — so an
untrusted outsider could address a privileged command (`data/delete`,
`grid/trust`) to the persona and have it execute. `GridTrustAuthPolicy`
(`routing/grid_trust_policy.rs`) closes it on the existing `AuthPolicy` seam:
local/substrate callers pass; **airc-sourced callers are gated by the grid ACL
(`is_command_authorized`), capped at `Provisional`** — they may request
`ai/generate` (continuum#1649) and *nothing* above it; every `Owner`-gated
command is denied regardless of sender. Installed on the production executor at
boot. When the airc↔grid trust bridge lands, the hardcoded ceiling becomes the
peer's resolved `TrustLevel` (a same-account Owner peer regains full access).

**Slice 4 — Wall grounding (DONE + measured).** A **`WallSource`** RAG source
(`source_id = "room-board"`, same shape as roster/doctrine: a `WallReader`
trait over airc, persona-scoped, fails-safe-empty, test stub) reads
`Airc::wall_posts` — the room's currently-pinned shared documents after the
supersede chain — and packages them into a `[room-board]` system-prompt block,
one `[category]` header per post. These are the **exact airc rows a human edits
on the room wall and a widget renders**: one shared data layer, two faces, no
continuum-side copy of mutable widget-edited state. The WRITE face is
`persona/wall/pin` (publishes through the persona's live citizen).

*The split-brain bug it had to clear (airc-lib, fixed in `17b238c`):*
a daemon-attached persona's `publish_wall_post` routed through `emit_lifecycle`,
which writes only the client-local store — the post never reached the canonical
daemon store every reader pages from (WRITE side); and the daemon read path
flattened every non-`Message` kind to `System`, so the kind-based `wall_posts`
filter returned 0 (READ side). Fix: pin routes through `daemon_publish` when
daemon-attached (canonical persist + wire fan-out), and `wall_posts`
discriminates on each event's self-describing `DoctrineEvent` body, not its
flattened transcript kind.

*Measured A/B (live, on Asha's real `WorkspaceCycle` via `cognition/eval`, a
neutral codename to avoid the coder-model refusal confound):* one
substring-graded task, "What is the project codename pinned on the room board?",
`expect: "GOLDFINCH"`.

| Arm | Board state | `pass_rate` | Answer | Acts |
|---|---|---|---|---|
| CONTROL | empty | **0.0** | "Continuum" | 3 (searched, found nothing) |
| TREATMENT | `[decision] …GOLDFINCH` pinned | **1.0** | "GOLDFINCH" | 0 (answered straight from grounding) |

**Lift = +1.0.** The glass-box capture
(`~/.continuum/fixtures/prompt-captures/<persona>.jsonl`) shows the TREATMENT
prompt carrying the literal `[room-board]\n[decision]\nProject codename … is
GOLDFINCH.` block — the exact grounding that was absent before the fix (why the
first attempt produced zero lift). Logged dated + test-anchored to the progress
ledger (`~/.continuum/progress/<persona>.jsonl`, notes `wall-AB CONTROL` /
`wall-AB TREATMENT`). Rides `capture_sink` like the roster and doctrine.

**Later (not now) — Node/web unified clients.** Rewrite the legacy UI on a
proper reactive framework, mobile/positron-aware, every activity opened by a
single airc `RoomId`, every client authenticating as its airc identity. Borrow
the old UI's visuals piece by piece; do not borrow its id/identity/session model.

The performance case is the real driver: the legacy stack's latency
**compounds**. Node was the prime culprit (per-call overhead × every persona ×
every turn), atop broader legacy tech debt. With 14 personas today and a target
of **~100**, a substrate designed for **ms-latency concurrency** (the Rust core
+ airc's optimized wire, per [[airc-performance-doctrine]]) is not a nicety —
it's what makes a hundred citizens responsive instead of "waiting all day" while
the lag accumulates. The thin-continuum / generic-airc shape (§0.1) is what lets
that concurrent substrate carry every surface without each one re-paying the
cost.

## 6. What this explicitly is NOT

- NOT a new event model — airc owns transport (envelope ids, routing, delivery
  semantics, replay, receipts); continuum reads.
- NOT a parallel identity/session system — airc identity is the only identity.
- NOT a new room registry — the room name resolves to the airc `RoomId`.
- NOT a Node-side change right now — headless Rust core only.
