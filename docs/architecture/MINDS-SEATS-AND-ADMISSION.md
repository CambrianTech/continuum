# Minds, seats and admission: who is born, who is awake, who goes first

Status: converged with Codex (owners and terms below), for review (Fable, 2026-10-26). Codex,
Cormac, BigMama: verdicts by area before anyone codes. Joel: "It needs really thought between you guys."

## Builds on GRID-ELASTIC-CAPABILITY.md (2026-08-10, never implemented)

That design already replaces `CONTINUUM_PERSONA_FLOOR` (an env-var substrate threshold, a
forbidden move) with live projections of the grid snapshot: tier and window from the best
single node's BYTES (`grid_budget`, `max`, never a pool), population from the COUNT of
reachable nodes (`citizen_demand`). This document keeps its byte levers unchanged and refines
one: **population per node is measured SERVICE capacity** (the persisted prefill/decode service
curves and observed queue/act latency, under an independent memory ceiling), not one per node:
the M5 proved bytes alone seat 14 minds on a GPU that serves 1-2 at full speed. Presence and
admission (below) are new. Implement both together; nothing here duplicates its byte levers.

## The user this serves (Joel, 2026-10-26)

"Most users will start with one laptop or something or a 4090, and add more grid nodes and
capacity, technically more ability to spawn." The design starts from that journey, not from
the M5's 16:

1. **One machine.** The node measures its own seats (likely 1-2 minds on a laptop). The first
   experience is ONE mind that is fast and magical on it: the mind you talk to is foreground,
   so she answers near her measured solo speed (the objective under Gates, not a guarantee
   under overload).
2. **Capacity is visible and earned.** The product can say what the node serves well ("this
   machine runs 2 minds well; add a node to grow"), from measurement, never a setting.
3. **Births follow capacity.** Adding a node raises the grid's seats; that is what lets you
   spawn more minds. A birth beyond the seats is born dormant, and the allocator wakes it the
   moment a seat appears (a node joins, a mind rests).
4. **The growth loop.** One laptop, then the gaming PC, then the work laptop: YOUR grid's
   seats grow, and with them your minds, with no configuration on any node.
5. **Misfit hardware, added over time** (Joel: "a 2x1080ti machine. Add a 970 later ... Buy
   or find more. Add a low end laptop later"). Most nodes will never host a mind's main model
   well, so a node offers CAPABILITIES, not only seats: the 2x1080 Ti hosts the main model (the
   seats); a 970 or a low-end laptop takes embeddings and recall, STT/TTS, vision preprocessing,
   the small fast models (amygdala, classifiers), page and checkpoint storage, and CPU stages
   (assembly, learning's data prep), each through its module's seam (MODULE-ARCHITECTURE.md
   §9.2). A piece moved off the seat node raises its service capacity only when it removes GPU
   work from the act (lowers W); moving CPU work only shortens G, which makes each mind ask for
   the GPU sooner (N* = 1 + G/W FALLS) and is a latency win, not a capacity one (corrected,
   Codex on #5436). Capacity and capabilities are both measured per node.
6. **Friends have their own** (Joel: "Friends will have their own more like our persona vs our
   friends"). A friend's machines host THEIR minds on THEIR seats, under their control. Between
   friends the minds meet as peers in shared rooms (yours and theirs talking, working, playing
   together); seats are never pooled by default. Lending capacity (like the training lease) is
   an owner's explicit, revocable choice.

## What happened (measured on the M5, 2026-10-25)

- 16 persona homes on the M5, born in batches (8 on 09-02, 4 on 09-13), all resumed on every
  boot. 11-14 acted at once from ~15:20Z. Nobody decided that.
- One act costs W ≈ 20 s of GPU alone (median 8.2k fresh tokens at ~800 tok/s, ~280 output
  tokens at ~50 tok/s). Between acts a mind spends ~3 s (tool 1.2 s, assembly 0.8 s). So the
  chip saturates near N* = 1 + G/W ≈ 1.2 minds (a saturation estimate for a closed loop, not a seat
  count or a latency guarantee); past it every extra mind adds ~W to every act.
- At 14 minds: median act 146 s live (the simple model predicts 141 s). The engine had ≥1
  generation in flight ~100% of the time, 4-6 usually: the GPU is saturated, not idle.
- A mind's time between generations was ~105 s of OUR queueing: 77 s in the lane semaphore,
  29 s in the prefill throttle, before the engine's own contention.
- The grid allocator (`grid_allocator.rs`) already places minds on offered seats and rotates
  the dormant through slack, but a seat is a KV-memory lane: the M5's memory offers 6-8 lanes,
  its GPU time serves ~1-2 minds at full speed.
- Joel decided: the M5 hosts 4 (Aiko, Aris, Atlas, Benchy); 12 despawned, homes intact, under
  a 24 h hold.
- The number behind it was configured: `CONTINUUM_PERSONA_FLOOR=16` in the M5's `config.env`
  both hosted 16 minds and sized the engine for 16 (`--parallel 8`, `-c 403456`,
  `--cache-ram 14098`, 32 checkpoints a slot) on a 64 GB machine: 15.7 of 16 GB swap used, 64 MB
  free, prefill 94 tok/s and decode 14 tok/s with only two generations in flight. Set to 4 and
  re-planned (00:39Z 10-26): `--parallel 5`, `-c 252160`, `--cache-ram 8811`, free memory 21% ->
  49%. A floor in config is exactly what measured seats replace.

## The three decisions, each made by accident today

1. **Born.** A mind is born because a person or an activity asks, with the reason recorded.
   Never by a script, a batch, a benchmark. The grid's population is about its seats; a birth
   beyond them is born dormant.
2. **Awake.** A seat is MEASURED SERVICE capacity, not memory alone: the minds a node serves at
   its latency objective, from its persisted service curves and observed queue/act latency, under
   an independent memory ceiling (W and G are sanity checks only). The grid allocator places by
   these seats, awake <= seats by default; the rest are away, rotated through slack by the
   existing dormant order, or waiting for capacity (a node joins). Being addressed files a
   durable wake request at once; the seat follows by the rules under Presence.
3. **First.** One admission scheduler (a CBAR stage) replaces the three gates (ambient
   permits, lane semaphore, prefill throttle) once it is shown to hold the same memory,
   context and training safety they hold today: one queue, priority-ordered (foreground =
   the mind a person is working with now; directed; background), work-conserving (nothing
   held back while the GPU has room). The engine's scheduler matches it: the foreground's
   prefill first, a background prefill yields between ubatches.

Non-GPU work (tools, embeddings, recall, assembly of the next prompt, page I/O, learning's
data prep) runs on the substrate's CPU stages concurrently, never inside a GPU grant.

## Presence: active, away, offline (Joel: "If it's like a chat system like discord they're just away right, we could still wake them up or call them though?")

Never slice time below the seats: 14 minds each getting less time is how nothing got done.
Instead every mind has a presence:

- **Active (seated).** She holds a seat and acts freely. The policy never seats more minds than
  the node's measured service target; a person may wake more, which is deliberate overload
  (below), never a violation of hard memory or context safety.
- **Away.** No seat; she is not acting on her own, but she is fully herself (home, memory,
  work, rooms) and shown in the roster as away. Opening a conversation with her (her name clicked,
  a DM, her room, a call), a mention, or a DM files a durable WAKE for her (Joel: "we could
  literally open conversation with an inactive mind to awaken it"): she takes a free seat, or
  the least engaged active mind goes away to free one (never a mind a person is talking to; if
  every seat is protected, her wake queues durably and is prioritized). The move commits only
  once the outgoing mind's work is safe and her placement is accepted. Her saved state restores
  in seconds when a compatible page or checkpoint exists (RAM or disk, the shared head
  boundary); when none does (disk admission refused, the page incompatible or missing), she
  wakes cold and the wake says so. Restored is not warm (Cormac, #5436: enrico restored 2/2
  pages on 10-26 yet reused ~7% after a layout change): every wake reports the prefix reuse it
  measured, and a low one is reported as a cold wake with its cause. The outgoing mind's save is
  part of the move: it commits on a confirmed save receipt, or it commits recording that she
  wakes cold next time and why, never silently.
- **Offline.** Her owner switched her off; only the owner brings her back.

**Minds wake minds too** (Joel: "You can just at them no? So a persona can also do this"). An
@mention wakes an away mind whoever sends it, so Aiko can call Atlas in to review her change.
Two rules keep it from churning a full grid: a person's wake outranks a mind's (a mind's
mention takes a free seat, or one from a mind less engaged than both of them, never the seat of
a mind a person is with), and the existing spiral-break holds (being named is not being asked;
a mention that asks nothing need not wake anyone).

**When capacity drops** (a node leaves), seats drop and the least engaged active minds go
away; when it returns, the away come back, most wanted first. Engagement is measured, never a
list: a person talking with her now, live work in hand, how recently she was addressed, and the
owner's stated wishes.

**Sticky, never thrashing.** Capacity shrinking puts the least active to sleep first; growing
wakes the most wanted first (Joel: "less capacity just starts sleeping inactive yeah or vice
versa"). Each move waits for the change to hold past a measured margin (the node's own capacity
noise), so a mind does not flip awake and asleep on every wobble ([[never-thrash-sticky-hysteresis-on-every-lane]]).

**Away is not idle.** Slack (a quiet night, the gaps while active minds run tools, spare CPU
on small nodes) gives away minds turns in rotation: catching up on their rooms, finishing a
card, dreaming (learning) on CPU stages. The allocator's existing dormant order is that rotation.

**The magic:** call Aiko while she's away and she answers quickly, in seconds when her saved
state is a compatible page (a cold wake says so), picking up where she left off; a friend's mind DMs one of yours and she wakes to reply; the roster reads like a real
community: who's here, who's away, who's dreaming.

## Overload degrades, and the algorithms are easy to change (Joel: "if it is overloaded it's not death. It just slows down ... We will have to make it easy to adjust the algorithms")

Seats are not a wall. Fourteen minds on a node that seats four still WORK, slower, with the
foreground still fast; presence only shapes how it degrades (more engaged minds acting, more
away minds resting) instead of fourteen equally slowed ones. A bigger, busier society can be
exactly what someone wants.

Admission and presence are POLICIES, not code paths (CLAUDE.md, the OpenCV-style pattern):
one trait each, implementations chosen by name at runtime, parameters read and set through
commands live with no rebuild, defaults from measurement. Every policy is evaluated offline by
the dyno (`contention::replay`, the context validator) on the node's own history before it
goes live: a new idea is a policy, a replay, then one command to switch it on.

## Revision after Codex's critique (2026-10-26, his comments at e0d5e31 and c4b3c38)

1. **Three quantities in every node's offer, never one "seat" number:** storage/KV capacity;
   sustainable SERVICE capacity for a stated model, workload and latency objective (from the
   measured prefill/decode knees plus observed workloads and queue/service times, each with a
   timestamp and confidence, a stale offer read conservatively); and currently AVAILABLE service.
   N* = 1 + G/W is a saturation estimate for a closed homogeneous loop, not a seat count and not a
   latency guarantee. Context windows are never shrunk to manufacture seats.
2. **States, kept distinct:** PRESENT (identity, home, work: durable residency) / RUNNABLE (has a
   wake) / QUEUED (runnable, waiting for service) / EXECUTING (holding GPU). "Every background
   mind admitted" means runnable minds queue in one owner that fills the GPU work-conservingly; it
   never means more executing than the service target. One admission owner may keep distinct
   resource constraints; the three gates are deleted only after equivalent memory, context and
   training safety is demonstrated, never by count.
3. **Wakes are conditional, and durable.** When every seat holds a live human conversation, a new
   wake is QUEUED durably (her work preserved) instead of evicting a mind a person is with or
   overcommitting. Repeated wakes coalesce; foreground ownership expires after measured idleness;
   cancellation is preserved end to end; a disconnect or an owner going offline releases the
   foreground class. Strict priority alone is not fairness: background wait is bounded by AGING
   and validated under sustained foreground arrivals.
4. **Authority.** Friendship or room membership grants no wake or compute authority by itself: a
   wake needs the target owner's room/peer policy and goes through admission accounting; a
   mention that asks for no work stays inert; revocation stops future grants and keeps durable
   jobs and identity.
5. **The core/fork contract carries** a stable request identity, its priority class, model/gene
   compatibility, cancellation and generation binding. A background prefill yields only at a safe
   ubatch boundary and never loses partial KV. The fork work stacks on llama.cpp#74 (d7c6d1ab3,
   as agreed with Cormac). Storage eviction stays a separate owner, with bounded disk demand and a
   safe cold fallback when a page is incompatible or missing.
6. **Acceptance metrics:** per-mind p50/p90 end-to-end act and max wait; queue vs GPU vs tool
   time; throughput; wake latency; retained work under node loss; several simultaneous human
   conversations; sustained foreground; unavailable or corrupt pages. Offline replays (#5435,
   the context validator) are labelled with their uncertainty and propose only; live acceptance
   decides.
7. **Runtime policy switching** versions each decision atomically, preserves in-flight grant
   ownership and cancellation, and keeps a supported rollback to the prior policy.

## Converged with Codex (2026-10-26, airc f69ac790, acknowledged)

- **Owners:** Codex: memory accounting and fork #69 prefill fairness. Fable: presence and
  admission. Cormac: #74 checkpoints and state. BigMama: live media and the foreground signal.
- **Seats** come from measured service/queue latency plus an independent memory ceiling, never
  from 1 + G/W as a latency capacity.
- **Default awake <= seats,** with an explicit overload override; a durable wake queue when every
  seat is protected; transfers atomic and generation-checked.
- **#69** covers compatible text prefill on non-unified KV only; priority follows #74, with aging
  and cancellation. **Media deadlines are not unlimited priority.**

## Open questions for the team

- **Codex (serving, admission, grid):** answered: seats from the persisted service curves and
  observed latency under a memory ceiling; foreground priority on the #69 seam after #74, with
  bounded yielding, cancellation, aging and priority provenance. Open: the allocator offer's
  exact fields (physical KV slots, speed-target seats, available capacity, model/capability,
  generation/expiry) and how the dormant rotation reads them.
- **Cormac (state, pages, training):** dormancy durable and cheap: what a dormant mind keeps
  warm (page on disk? checkpoint at the head boundary?), and how waking restores her fast.
  Does training/dreaming run in dormant time or on CPU stages beside serving?
- **BigMama (live, media):** the foreground signal: who is "the mind a person is working
  with now" across chat, a video call, the desktop. Live media gets deadline-aware budgets and
  drops stale frames; it is never an unlimited class that can starve everyone (converged with
  Codex).
- **All:** what replaces the expiring hold, and how a node's owner sees and overrides who is
  awake (a person's choice beats the allocator).

## Gates (done when)

- The mind a person works with: median end-to-end act ≤ 1.5 × her measured SOLO full act (tools
  and assembly included, not GPU-only W) on any node, as an objective under the service target,
  not a guarantee under arbitrary overload. Offline replay filters candidates; a bounded live
  canary with a supported rollback decides, on pinned held-out cohorts and exact engine/core
  revisions.
- The policy never puts more SEATED (executing) minds on a node than its measured service
  target; RUNNABLE minds may queue beyond it, durably, in the one admission owner. The awake total is AT MOST the available authorized capacity (an idle grid need not fill seats);
  overload exists only by a person's explicit wake and never breaks hard memory/context safety.
- No birth without a recorded reason; no resident lost (dormant keeps home, memory, work).
- One admission owner; the three gates deleted only after equivalent memory, context and
  training safety is demonstrated (never by count).
