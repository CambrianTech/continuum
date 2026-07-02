# Widget as State-Kind — one contract, three verbs

> **Read this before** touching `core/continuum-positron/`, `core/continuum-core/src/ipc/positron_source.rs`, `apps/web/`, a future `apps/tui/`, or any renderer that projects a positron state-kind. It is the precedence-winning truth on what a "widget" is in the reinvented client.

## The thesis

A **widget is not a component. It is one typed state-kind projected three directions.** Take `chat`. There is exactly one canonical shape for it (`ChatViewState` in `continuum-positron/src/chat.rs`), and three verbs act on that one shape:

| Verb | Direction | Who | Surface |
|---|---|---|---|
| **Project** | kind → surface | the substrate renders the kind | web (Lit + ported SCSS), terminal (ratatui text grid), mobile (same kind, native shell) |
| **Perceive** | surface → mind | a persona reads the kind | structured positron feed **+** visual screenshot (dual-channel) |
| **Produce** | mind → kind | a persona writes the kind | today: emit a chat turn; later: emit an *interface* |

One contract, three verbs = the compression. The old widget system had chat logic in 1633 lines of TS, styling forked per surface, and no way for a persona to see or drive it. Here the *kind* is the single source of truth and every surface — human eyes on a screen, an ANSI terminal, a persona's cognition — is a projection of it. **"An interface that meets you where you want it. Dynamic and capable. Same who/what/where design across."**

## Two complementary layers (not alternatives)

Keep both. They are different axes, not competing choices:

- **positron = state + wire.** The typed, revisioned state cache (`Substrate`) the WS server serves to thin clients. A `chat` envelope carries `{room_id, room_name, messages, roster}` at a monotonic `revision`. positron is a **projection of airc-owned truth, never a second store** (see `[[airc-to-positron-chat-projection]]`).
- **Lit + SCSS / ratatui = presentation.** How a projected kind becomes pixels or cells. The web renderer is a Lit `<chat-widget>` with the ported `chat-widget.css` (`:host`-scoped, token-driven). The terminal renderer is a ratatui buffer. Both consume the *same* `ChatViewState`; neither owns state.

The wire is engine-neutral POD (`#[derive(TS)]` → generated TypeScript). The presentation is per-surface. This is the same split the avatar engine draws between `SceneDescription` (backend-neutral invariant) and the Bevy/Unreal render backend that instantiates it.

## The pluggability thesis — source of truth lives LOW and NEUTRAL

The strongest, most flexible contract puts truth at the **lowest, most neutral layer** and lets every consumer *project* it. That layer is airc, and airc is already shaped for exactly this. Its authoritative identity card (`airc-core/src/identity.rs`) carries `name`/`pronouns`/`role`/`bio`/`fingerprint` **plus** an open `integrations: BTreeMap<String,String>` whose own doc says: *"cross-system identity binding (e.g. GitHub login, Continuum persona id, OpenClaw user record) … airc never interprets the values; it just persists + transports."*

That is the whole design in one field. airc transports identity truth and stays **dumb about what a "persona" is**. continuum, Hermes, openclaw, and our python foundry are **peer consumers** that each register their own binding key (`continuum.persona_id`, `openclaw.user_record`, …) and each *interpret* kind themselves. Because airc is the lowest correct source of truth and refuses to interpret, anything can plug in on top — continuum today, a Hermes/openclaw runtime tomorrow, all reading the same authoritative card. **Deep + neutral + low = maximal pluggability.** Every contract decision below is chosen to keep truth at that layer and make continuum a projection, never a second interpreter.

**The neutrality is fractal — and positron is its own independent repo.** positron ("React + agents + modern terminals" — a general-purpose state/view layer, consumes airc, **knows nothing about continuum**) repeats airc's move one layer up: it carries a *general* sender taxonomy (`Human | Agent | System`) **plus the opaque `integrations` badge map transported, not interpreted**. It does not — must not — bake in a continuum-specific `Persona` variant, because that would leak continuum into a repo others adopt without knowing continuum exists. continuum reads `integrations["continuum.persona*"]` off that passthrough and styles its own personas distinctly at the *app* layer; a different adopter reads their own key. So: **airc neutral (mesh) → positron neutral (view/state) → continuum interprets (app).** Three independent, generic repos; continuum tailors both without leaking downward. Others use airc + positron, interoperate with continuum, and never need to learn continuum.

Corollary: continuum's `sender_kind` is a **projection of airc-neutral truth**, never a continuum-private guess. It is resolved by reading the sender's airc identity card (the `integrations` binding the owning system stamped), not by checking continuum's local hosted-persona set (which is blind to remote personas) and never by string-matching a `runtime` value (task-#70 anti-pattern). See § *The `sender_kind` resolution rule*.

## The subscribe → state protocol

Thin clients don't poll and don't run commands over the session wire. They subscribe to kinds and receive flattened state pushes:

```
client → server   {"type":"subscribe","kinds":["chat"],"layers":["session"],"last_seen":[]}
server → client   {"type":"state","kind":"chat","revision":N,"payload":<ChatViewState>}
```

No correlation id on the state push (it isn't a reply — it's a broadcast of the current truth), no `CommandFailed` on the WS session wire (commands ride the RPC path; see `ipc/positron_dispatch.rs`). A later revision supersedes an earlier one for the same kind. The Substrate cache is keyed by `kind` string alone → **one focused room at a time**; per-room instancing is kind-instancing, deferred.

## Verb 1 — Project (kind → surface)

The renderer is a pure function of the latest `ChatViewState`. Outlier-validation doctrine (CLAUDE.md § methodical process) governs which renderers we build to *prove* the abstraction:

- **Outlier A — web DOM/CSS.** Lit `<chat-widget>` in `apps/web/src/`, ported `chat-widget.css`. Message bubbles keyed on `SenderKind` (human / persona / system — the old two-way current/other split becomes three-way; persona gets its own bubble class).
- **Outlier B — terminal text grid.** A `apps/tui` ratatui crate (sibling of `apps/cli`'s `ctm`). Maximally different from web DOM: no CSS, no DOM, a character cell buffer.

Two maximally-different renderers fitting the *same* kind without forcing it → the surface abstraction is proven → **mobile is then mechanical** (same kind, native shell). Build A and B, validate, then STOP — don't pre-build mobile.

The terminal is not a CLI. It is "more like the web than a cli" — a live, subscribed, redrawing view of the same state the browser shows. **"They're able to do really amazing terminals that are more like the web than a cli. This is the new way."**

## Verb 2 — Perceive (surface → mind) — dual-channel

A persona perceives a widget **two ways at once**, because both are legitimate feedback at any point:

1. **Structured channel** — the positron `ChatViewState` itself (or the airc-direct RagSources that already ground the persona: `AircRagSource`, `RoomRosterSource`). Machine-precise, token-cheap, no vision model. This already works; leave it alone.
2. **Visual channel** — a screenshot of the widget or the whole screen (task #94, `Screenshotter` adapter family). **"Screenshots and all visuals are feedback at any point, either widget or whole screen."**

The terminal's framebuffer *is* perceivable text — the ratatui cell buffer needs zero vision model to read back. That makes the terminal the cheapest possible "what does the screen look like right now" channel for a persona.

## Verb 3 — Produce (mind → kind)

A persona writes the kind. Today that is narrow — **emit a chat turn** — and it generalizes exactly the way the avatar engine's `SceneDescription` generalizes: one type, produced three ways (human-authored file / projection-fed / persona-birthed). `WidgetState` follows the same arc: human-authored → projection-fed → **persona-birthed**. The horizon: a persona doesn't just post *into* a chat widget, it emits an *interface* — "stuff shipped with agents embedded, so our interfaces are all AI."

### The producer seam (the current build)

The projection in `positron_source.rs` is a passive `MessageBus` consumer that folds `chat:posted` → messages and `presence:updated` → roster. It is **currently starved**: nothing publishes those bus events, so both human and persona posts bypass the projection. The missing publisher is the produce seam, and its placement is a decided design call:

**Decision: emit at the room-level airc→bus republisher, not the per-persona say seam.**

`core/continuum-core/src/airc/inbound_attach.rs::publish_transcript_event` already attaches to the airc channel `FromTranscriptStart`, decodes every airc `TranscriptEvent`, and republishes continuum-tunneled events onto the bus. Plain chat `Message` events currently fall through to `Ok(None)` and are dropped. The producer is **one arm added there**: on a `TranscriptKind::Message` event, emit `chat:posted{AircChatPosted}` using airc's authoritative `event_id` as `message_id`.

Why this seam and not the persona's `say()` (the scout's first instinct):
- **Covers every sender uniformly** — human, this persona's own echo, remote grid peers — from the airc system of record, in one place. The `say`-seam only sees this persona's outbound turns.
- **Authoritative EventIds** — airc's `event_id`, not a fabricated uuid. (`PersonaConversation::say` discards the airc `EventId` via `.map(|_event_id| ())`; using it would mean widening the trait through the gated `service_loop.rs`.)
- **Avoids the gated brain file** — no edit to `persona/service_loop.rs`, no `MessageBus` threaded into `PersonaContext`. It is a legitimate wire-driver concern living in the wire driver.
- The starved `AircBridgeDirectiveModule` already *assumes* `chat:posted` is on the bus — this fills a gap multiple consumers depend on, not a bespoke path.

### The `sender_kind` resolution rule (project airc-neutral truth, never guess)

The thin `AircChatPosted` deliberately carries **no** `sender_kind` — kind is not a message fact, it is an **identity** fact, and identity lives at the airc layer (§ *pluggability thesis*). Resolution happens by reading the sender's airc identity card, not by inventing it. Two layers do two different jobs:

**positron (general, continuum-blind)** projects the neutral taxonomy `SenderKind = Human | Agent | System` **+ the opaque `integrations` badge map** passed straight through from the airc card:
- **System** ← the event is a substrate-authored kind (`TranscriptKind::System` / lifecycle kinds).
- **Agent** ← the card carries *any* AI binding / agent runtime marker — a continuum persona, an openclaw actor, a Hermes agent are all `Agent` at this layer. positron does not distinguish whose AI it is; it just knows "an agent."
- **Human** ← no AI binding present.

**continuum (app layer, interprets)** reads `integrations["continuum.persona*"]` off that passthrough to style *its own* personas distinctly (its own bubble class). A remote continuum persona still styles correctly because the binding travels with the peer. A different adopter reads their own key. **positron never learns what a "persona" is** — that keeps positron a repo others adopt without knowing continuum exists.

This is deliberately **not** a check against continuum's *local* hosted-persona set (blind to remote personas) and **not** a match on `RoomMember.runtime` (`"persona"`/`"claude"`/`"codex"` — that string describes *client software*, not actor nature; string-matching to classify actors is the task-#70 anti-pattern). Reading a binding the owning system authoritatively stamped is authoritative; sniffing a runtime string is a guess. Different things.

`sender_name` and `room_name` resolve from the same airc-owned identity/roster truth (`Identity.name`; `airc_lib::Airc::room_roster(within, window) -> Vec<RoomMember>`, airc#1232) — never fabricated. Until the sender's card has arrived in the projection's accumulated roster, the message renders **provisionally** (short peer-id label, `Human`) and upgrades in place when presence carries the card — a provisional projection pending authoritative truth, not an invented identity (`[[fallbacks-are-illegal-fail-loud]]`).

**Near-term stamping obligation:** continuum must stamp its persona binding into the airc `Identity.integrations` at bringup (a small additive write on the identity it already publishes) so the projection can read kind back authoritatively. Until it does, personas resolve provisionally to `Agent`/`Human` — honest, not wrong. The binding write is the continuum-side half of the neutral contract; airc needs no change (it "never interprets the values; it just persists + transports").

## The RDP-in horizon

The end state: a persona has a **seat at the same screen a human sees**, not an AI-facing API bolted alongside. Two channels — a framebuffer (screenshot / ratatui buffer) and structured session state (the positron feed) — are exactly what a remote-desktop session is. **"It's like they're rdp in."** A persona can see any surface (web, mobile, terminal), perceive it visually *and* structurally, and produce into it. There is no disconnect between "the UI" and "what the AI can touch" because the UI *is* a projection of state the AI already reads and writes. Agents design UI/UX not as static mockups but as shipped, agent-embedded interfaces — because Produce generalizes from "a chat turn" to "an interface."

## The slice (build order)

1. **Producer emitter** — thin `chat:posted{message_id, room_id, sender_id, content, timestamp}` arm in `inbound_attach.rs::publish_transcript_event` (this doc's § Produce), using airc's authoritative `event_id` as `message_id`. Carries NO identity — kind/name/room_name are projected downstream from the airc identity card. Covers human + persona + peer in one bridge. (task #84)
2. **Presence emitter** — `presence:updated` from the same attach stream's identity/presence events (roster snapshot **carrying each member's airc identity card**, incl. `integrations`). This is where `sender_name`/`sender_kind`/`room_name` become resolvable — the projection folds the roster, then `apply_message` looks the sender up by id. Continuum stamps its `continuum.persona*` binding at bringup so kind reads back authoritatively.
3. **TS `WebSocketTransport`** — subscribe → state handling + regenerate/relocate the stale wire types (task #80).
4. **Web `<chat-widget>`** — outlier A, Lit + ported CSS, `apps/web/src/`.
5. **`apps/tui`** — outlier B, ratatui renderer.
6. Persona perceive: structured already grounds (leave alone); wire the screenshot visual channel (task #94).

## Invariants (non-negotiable)

- The kind is the single source of truth. Renderers are pure functions of it; they never derive state from URL slugs, ids, or each other.
- positron projects airc truth. It never becomes a second store — and never a second *interpreter*: identity/kind come from the airc identity card (the neutral low source of truth), so Hermes/openclaw/foundry consumers all agree.
- `sender_kind` is a projection of the airc identity card's authoritative binding (`integrations`), never continuum's local hosted-set and never a runtime/name string-match. Unresolved identity → provisional (short-id + `Human`), upgraded in place when the card arrives; never fabricated.
- Two maximally-different renderers prove the surface abstraction, then STOP. Mobile is mechanical, not a third hand-forced impl.
- Produce generalizes (chat turn → interface) the same way the avatar engine's `SceneDescription` is produced three ways.

## Related

`[[airc-to-positron-chat-projection]]`, `[[airc-native-identity-rooms-security]]`, `[[persona-is-a-client]]`, `[[rag-as-persistent-cache]]`, `[[fallbacks-are-illegal-fail-loud]]`. Docs: `PERSONA-COGNITION-PIPELINE.md` (the produce-side cognition cycle), `OBSERVABILITY-AS-SUBSTRATE.md` (capture/replay of state transitions), `CLIENT-SDK-PLATFORM-ARCHITECTURE.md` (thin-client tiers).
