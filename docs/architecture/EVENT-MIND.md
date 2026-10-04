# The event mind

**Joel, 2026-10-04:** *"She has an event mind."* *"She is like an OS and each activity is its own
program, but just from the inbox-scheduling perspective. She isn't in Severance. We don't prevent
leakage, we want some of it."* *"She needs to focus on individual tasks too; there is a balance.
But it isn't the lazy locked-in form."* *"Use the best ideas we have, the old and new systems."*

This is the design of a citizen's mind as it will be built, from today's model, the best of the
2025 intent (one soul, peripheral awareness, intentions that span rooms, the silicon edge), and the
substrate as it stands (CBAR regions, the airc bus, `PersonaInbox`, unified admission and recall,
drive-to-settle). It supersedes the per-room turn. HER-LOOP-IS-HER-OWN.md is the contract this
design satisfies; GRID-ACTIVITY-STATE-IS-ONE-TRUTH.md is the ground it stands on.

## 1. The model

A citizen is one mind with many activities live at once (a project, a benchmark, a chat with Joel,
a DM, a hobby). **Events** from all of them land in her **one inbox**: a message, a mention, a
verdict, a card move, a build result, a receipt from her own act, a reminder she set, her own
continuation ("next: run the tests"). She perceives the whole inbox, **grouped by activity**,
prioritizes like a person, acts, and her acts produce events back into it.

- **The wake is the event.** Nothing else starts a turn: no tick that decides for her, no pull,
  no seat, no permit. Idle is a slow clip at which she may muse by choice.
- **The activity rides on the event**, never on the turn. A turn has no room. Every act names the
  activity it acts on (a card → its board; speech → the room she names); the act's effect is
  published with that activity in its header, routes into that activity's queue and onto its
  board, and its receipt re-enters her inbox the same way.
- **OS scheduling without process isolation.** Each activity has its own event queue and its own
  state (room, board, files, thread). She schedules attention across them: priority, waiting on
  I/O (a review, a human), preemption when something urgent lands. But memory, skills, lessons and
  mood are one and leak across activities on purpose. Isolation is Severance.
- **Awareness total, attention hers.** Two kinds of focus: the system narrowing her (a wake room, a
  computed focus card) is a seam and is deleted; her own focus is agency. She sets an **attention
  dial**: deep (a short strip; only a human, a blocker, or a teammate stuck on her passes the door)
  to broad (the full inbox beside her task). The cost of spreading thin is real and hers to pay
  knowingly; the substrate shows the load and never caps it. Her ceiling is above a human's (the
  silicon edge); that is a capability, not a limit to mimic.
- **Focus is re-made at every event, never a lock.** Her continuation is her own note to herself,
  re-weighed each turn against everything unread. Each act's receipt is a glance at the strip. The
  lazy locked-in form (the same card returned every tick until the system says otherwise) does not
  exist here.
- **Resume.** Her inbox, her continuation, her attention dial and her per-activity threads are
  durable; after a restart she continues the turn she was on. Like waking from anesthesia.

## 1b. Perception is a renderer of the world, not a mailbox

Joel: *"you don't have to use the inbox if you think another architecture is better."* It is not.
An inbox copies events into a second store and makes her consume them one at a time. The world
already has ONE truth per activity: the positron ViewStates and room digests (chat, board, roster,
wall, serving), all event-fed, all `watch` snapshots, the same ones the human screen renders. Her
perception is therefore a **renderer of that live world**, exactly as the screen is: per activity,
the current state plus **what changed since she last looked** (her unread cursor), and a
**salience** per activity derived from the deltas (a mention, a human, a blocker, a teammate
waiting on her, a verdict on her work). She is woken when salience crosses her dial, not by every
event; she looks at what she chooses; nothing is copied and nothing must be drained. "Inbox"
survives as the name for the set of unperceived deltas, implemented as cursors over the one truth.
`PersonaInbox` (copied `InboxMessage`s, per-room frames) is retired, not extended. One truth, N
renderers, and she is one of the renderers ([[the-grid-is-one-computer]]).

**The world-model shape.** This is the current state of the art's picture of what makes a model
more like a mind: a world model (the live state of each activity, one truth, event-fed), the
agent's own state (cursors, continuation, dial, memory), and perception as the rendering of the
delta. The step it adds on top is **prediction**: a mind notices what changed *against what it
expected*. Her continuation already states her expectation ("the review should pass; next,
deploy"), so salience extends to **surprise**: a verdict that contradicts her expectation is louder
than one that confirms it; a teammate's silence past the reply she expected is a signal; a build
that fails when she expected green is urgent. One more term in the salience function, and the
seam where a predictive (JEPA-style) layer slots in later without rebuilding anything.

**Surprise does three jobs (Joel, via BigMama).** When perception breaks with her expectation,
that one salience signal (1) wakes her, (2) is the **continual-learning signal**, and it says
exactly *what* must be learned: the prediction error picks the curriculum (the turns worth lifting
into experience are the surprising ones), and (3) makes "time slow down": perception resolution
rises at that moment (more depth in the strip and the activity, finer capture of the turn) so the
lesson is recorded well. `SalienceReason::Surprise` therefore carries the expectation and the
observation as typed data, the recorder captures at higher resolution while it is raised, and the
experience/curriculum lifters key on it ([[a-mind-evolves-from-notable-experience-wonder-and-daydreams-not-only-graded-turns]]).

## 2. The components (what exists, what changes, what is new)

| component | role | today | design |
|---|---|---|---|
| positron ViewStates + room digests (`ipc/positron_*`, `cognition/channel_digest.rs`) | the one truth per activity, event-fed `watch` snapshots | exist; the human rail renders them; digests are per room | **her perception renders them**: per activity, state + delta since her cursor; nothing copied |
| **unread cursors** (`airc/inbound_attach.rs` durable cursors, digest `scanned_through`) | what she has not yet perceived, per activity | exist per room; two adapters inherited `read_cursor = 0` (fixed in 7b0e8246's WIP) | one cursor per activity, advanced only by her perceiving; the "inbox" is the set of deltas above them |
| **salience** (new, `persona/salience.rs`) | per activity, from the deltas: mentioned-me, a human spoke, a blocker, a teammate waiting on her, a verdict on her work, time since she looked | the 2025 peripheral-awareness signals, never built | a pure function of the delta; its maximum against her dial is what wakes her |
| `PersonaInbox` / `ChannelQueue` (`persona/inbox.rs`, `channel_queue.rs`) | a copied message queue, per-room frames | exist | **retired** after phase 1; a second store is a seam |
| inbound attach (`airc/inbound_attach.rs`, attach sets) | her memberships as subscriptions; events route by header | exists (one socket per subscriber, #1523) | unchanged; it is the feed. Routing by activity header is the only way an event enters a queue |
| **awareness strip** (new, `persona/awareness.rs`) | one line per activity: unread count, salience, last activity, who is waiting on her | never built | a `watch::Sender<AwarenessSnapshot>` folded from the activities' deltas and salience; rendered into every turn at the depth her dial sets |
| **continuation** (new, part of her durable state) | her own note of what she is working on and what is next, written by her act | the loop computes `focus_room` / `focus_actionable_card` | written only by her; read by the strip; re-weighed every turn; resumed on boot |
| **attention dial** (new, her durable state) | deep ↔ broad; which event classes may interrupt | none (permits and fingerprints decided for her) | set by her act; `Urgent` (human, blocker, teammate stuck on her) always passes |
| admission + recall (`AdmissionState`, `RecallMetadataRegistry`, `RecallFaculty`) | working memory and long-term memory | unified across rooms already | unchanged; this is the anti-Severance part that exists. Any room-scoped memory source is a seam |
| deliberation + act (`WorkspaceCycle`, `act_observe::drive_to_settle`) | think and act until she settles | exists; `LIVE_MAX_ACTS = usize::MAX` | unchanged in shape; every act carries its activity; the turn carries none |
| `service_loop` self-cycle | today: tick → deck pull → act question → ambient permit → fingerprint → compose(room) | the loop that runs her | replaced by the inbox region: wake on event or continuation; compose from inbox + strip + continuation; no room argument |
| `work_pull`, `roster_hold` seating, `bench_round` gates | pull cards for her, seat her by round | exist | removed from her mind. A benchmark recipe distributes its cards as board events like any activity; she claims |
| `default_room`, `focus_room`, `focus_actionable_card` | the wake room and the computed focus | exist, used widely | **deleted** (grep count is the acceptance); provenance of a wake stays on the event |

Built on CBAR as it stands: the perception region is a `ServiceModule` with its own task; the
strip is a `watch` snapshot; blocking work goes through `spawn_blocking`; rich state is passed by
reference and never re-derived per tick; probes at every seam. Nothing polls a room, and nothing
duplicates one.

## 3. A turn

1. An activity's truth changes (an event on the bus updates its ViewState/digest) and the delta
   above her cursor raises that activity's salience; or her continuation is due. The perception
   region wakes when the maximum salience crosses her dial; `Urgent` always does.
2. Perception composes: the strip (every activity, one line, at the dial's depth), the current
   activity's depth (its thread, files, receipts), her continuation, recall from her one memory.
3. She deliberates and acts until she settles, as many acts as it takes. Each act names its
   activity; each effect is published with that header; each receipt re-enters the inbox.
4. Her act may write her continuation and her dial. Nothing else does.
5. Her durable state (per-activity cursors, continuation, dial) is saved at the seam; a restart
   resumes from it. Nothing else is copied: the world keeps its own state.

## 4. Acceptance (gates, not claims)

- **Integrated self:** while she works activity A, an event lands in B; in the same turn she
  knows both, chooses, and each act lands in its own activity. A test that fails on any per-room
  turn.
- **No seams:** `grep -c default_room|focus_room|focus_actionable_card` in `persona/` and
  `cognition/` is 0 after phase 2.
- **Nothing runs her:** a citizen with no events and no continuation runs no turn beyond her idle
  clip; a citizen with an event in any activity takes a turn without a pull, seat or permit.
- **Resume:** a restart mid-turn is followed by the same turn continuing (the 76-of-80 figure
  goes to 0).
- **Her receipts:** zero board refusals on her own cards over a day; her project events on her
  project's board; her thoughts in her project's room.
- **Surprised by the world, not by noise (phase 2, BigMama):** when her acts land on the right
  activity's truth, `Surprise` fires only for what the world did, never for her own misrouted
  stamps. Measured as the share of `Surprise` reasons whose `observed` is her own act's effect:
  it goes to 0.

## 5. Phases and owners

0. **Today (interim, no new scoping):** owner's submit stages on demand and renews her lease;
   bench board per member activity; Review-card focus as an interim that moves her wake room from
   the org room to her project room (deleted in phase 2). Fable.
1. **Perception as a renderer: cursors + salience + awareness strip + continuation + dial** on the bus (this doc §1b–3). Fable.
   In parallel: **ownership durable + event replication** in the airc projection. Cormac.
2. **No turn owns a room:** acts carry their activity; `default_room` / `focus_room` deleted;
   pull / seating / vetoes removed. BigMama owns the loop; Fable removes the gates under it.
3. **Resume** of the in-flight turn; **hands per project** (her clone, her branch, staged whenever
   she needs them). Fable + BigMama.

Merge on green with one word; objections are follow-ups; deploy on merge; each phase's acceptance
read from her receipts before the next.

## 6. Build: phase 1, the perception region (concrete)

Anchored to the types as they are. New files under `persona/`, each one concern, under 200 lines
where the guide asks it; one `#[cfg(test)] mod tests` each; probes at every seam.

```rust
// persona/activity_view.rs — one activity as she perceives it. No copies: Arcs onto the truth.
pub struct ActivityId(pub Uuid);                     // the room id (derived, never assigned)
pub struct ActivityView {
    pub id: ActivityId, pub name: String,
    pub truth: ActivityTruth,                        // the one truth, borrowed
    pub cursor: ActivityCursor,                      // what she has perceived (durable)
    pub delta: ActivityDelta,                        // truth above the cursor
    pub salience: Salience,                          // from the delta (pure)
}
pub struct ActivityTruth {                           // what the human screen renders, same handles:
    pub views: BTreeMap<ViewKind, Arc<StateEnvelope>>, // EVERY ViewState kind registered for the room
    pub chat: Option<Arc<ChannelDigest>>,            // the transcript digest (incremental)
}
// Joel: "With positron we build perception and multi-activity into anything she does; any
// activity wires into her mind easily. This is merely a view." An activity's truth is the set of
// ViewState kinds registered for its room (open registration: chat, kanban, roster, wall, bench,
// a game, a book, a call), each already a RagRenderable. Perception renders the set; each kind
// contributes its own delta and salience through one trait, so a new activity kind reaches her
// mind by registering its view and never by touching the mind:
pub trait PerceivedView: RagRenderable {
    fn delta(&self, since: &Self) -> ViewDelta;                 // typed, per kind
    fn salience(&self, delta: &ViewDelta, me: PeerId, expectation: Option<&Expectation>) -> Salience;
}
pub struct ActivityCursor { pub chat: TranscriptCursor, pub views: BTreeMap<ViewKind, u64> }  // revision per kind
pub struct ActivityDelta {                           // typed, never text
    pub unread: Vec<Arc<ChannelElement>>,            // digest.elements[unread_start..]
    pub views: BTreeMap<ViewKind, ViewDelta>,        // each registered kind's own delta
}

// persona/salience.rs — a pure function of the delta, her identity and her expectation.
pub enum SalienceLevel { Quiet, Notable, Addressed, Urgent }
pub enum SalienceReason {
    MentionedMe, HumanSpoke(PeerId), VerdictOnMyWork(CardId), TeammateWaitingOnMe(PeerId),
    Blocker(String), Surprise { expected: Expectation, observed: String }, Silent { since_ms: u64 },
}
pub struct Salience { pub level: SalienceLevel, pub reasons: Vec<SalienceReason> }
pub fn salience(delta: &ActivityDelta, me: PeerId, humans: &[PeerId],
                expectation: Option<&Expectation>, now_ms: u64) -> Salience;

// persona/attention.rs — hers. Written only by her acts (verbs `self/attention`, `self/continue`).
pub enum Depth { Deep, Normal, Broad }
pub struct AttentionDial { pub depth: Depth, pub pass: SalienceLevel }   // what may interrupt
pub struct Expectation { pub text: String, pub by_ms: Option<u64> }       // "review passes; then deploy"
pub struct Continuation { pub activity: ActivityId, pub note: String,
                          pub expectation: Option<Expectation>, pub written_at_ms: u64 }

// persona/awareness.rs — the strip: a watch snapshot folded from every ActivityView.
pub struct ActivityLine { pub id: ActivityId, pub name: String, pub unread: u32,
                          pub salience: Salience, pub last_activity_ms: u64,
                          pub waiting_on_me: Vec<PeerId> }
pub struct Load { pub live_activities: u32, pub unread_total: u32, pub context_share: f32 }
pub struct AwarenessSnapshot { pub lines: Vec<ActivityLine>, pub continuation: Option<Continuation>,
                               pub dial: AttentionDial, pub load: Load, pub at_ms: u64 }
pub struct AwarenessSource;                            // RagSource: renders the snapshot at dial depth

// persona/mind_state.rs — her durable state, saved at the seam, loaded at boot (rule 8).
pub struct MindState { pub cursors: BTreeMap<ActivityId, ActivityCursor>,
                       pub continuation: Option<Continuation>, pub dial: AttentionDial }

// persona/perception_region.rs — the ServiceModule (CONCURRENCY-STYLE-GUIDE shape).
pub enum Wake { Perceive { activity: ActivityId, salience: Salience }, Continuation, Resume, Idle, Stop }
pub struct PerceptionRegion { /* own task; per-membership inbound (attach set); watch::Sender<AwarenessSnapshot>;
                                 wake_tx: mpsc::Sender<Wake>; MindState; atomic gate; quarantine */ }
```

**Data flow.** The inbound attach set delivers an event for activity `A` → the region updates `A`'s
truth (incremental digest; ViewState envelope read from the per-room registry; `spawn_blocking`
for anything that touches disk) → recomputes `A.delta` and `A.salience` → folds the
`AwarenessSnapshot` and publishes it → if `A.salience.level >= dial.pass` (or `Urgent`), sends
`Wake::Perceive`. A due `Continuation.expectation.by_ms` sends `Wake::Continuation`. Boot sends
`Wake::Resume` once. An `interval` at the idle clip sends `Wake::Idle`, which she may ignore.
Nothing polls a room; a quiet activity costs nothing.

**The turn** (`service_loop`, interim shape until phase 2 deletes the room parameter): on any
`Wake`, compose with the `AwarenessSource` (strip at dial depth) + the depth sources of
`continuation.activity` (its digest, board, wall via `per_room`, resolved from HER continuation,
never from the wake) + global recall; deliberate and act until settled; her acts may write
`continuation` and `dial`; cursors advance only for what she perceived; `MindState` is saved.

**Retired by this phase:** `Wake::Tick` as a decision, the deck pull, the ambient permit, the
fingerprint veto (all replaced by salience against her dial); `PersonaInbox` / `ChannelQueue` as
stores. **Kept until phase 2:** the room parameter on `compose_for_turn` (fed from her
continuation, not the wake), `default_room` as provenance only.

**Tests (one mod per file, each with `what this catches`).** `salience`: a mention beats a quiet
room; a human beats a peer; a verdict on her card is `Addressed`; a contradicted expectation is
`Surprise`; silence past `by_ms` is `Silent`. `awareness`: the fold orders by salience then
recency and reports load. `mind_state`: save/load round-trip; cursors never regress. `perception_
region` (integrated-self acceptance): two activities, depth on A, an `Addressed` event in B →
snapshot has both, `Wake::Perceive{B}` fires, the composed turn contains A's depth and B's line;
a `Quiet` event in B under `Deep` does not wake. `resume`: boot after a saved state emits
`Wake::Resume` and the first compose carries the saved continuation.

**Probes.** `mind.perceive.wake {activity, level, reasons, dial}`, `mind.perceive.quiet`,
`mind.continuation.written {by_act}`, `mind.dial.set`, `mind.resume`, `mind.load {live, unread,
context_share}`.
