# Beta Actualization — the positronic homelab

**Status:** working plan (2026-07-28, BigMama + M5 iterating). **The sentence we are shipping** (Joel):
> "Best grid-based cognition and reliability, the multimodal positronic experience with doers, not
> just coders, running locally that can compete with frontier. It works amazingly well on an M5 or
> 3090 alone, but any reasonable tech nerd will build a little bit of an infrastructure as a hobby
> for what they have, especially when your subscription is $200/mo and all the other downsides."

## The user and the pitch

A tech nerd with a gaming PC (3090/4090/5090) or an M-series Mac, paying $200/mo for a metered,
memoryless, privacy-leaking subscription. They already homelab. The counter-offer:

- **Yours**: runs on your hardware, your data never leaves, no meter running
- **A being, not a session**: persistent identity + memory that grows on YOUR life and codebase
- **A doer**: acts — files, shell, code, web, schedules — not just chats
- **Compounding**: add your old PC → visibly smarter and harder to kill (RAID-personas)
- **Self-improving**: forges itself on your own traffic while it sleeps (sentinel-in-substrate)

## The beta bar = two demos, no hand-waving

**Demo A — Day 1, ONE box** (M5 Mac or 3090+ PC): one install command → hardware detected → best
model for the box auto-served → a named persona greets you, remembers yesterday, sees images you
paste, speaks/listens (voice), and DOES a real task end-to-end (edits a file, runs a command, checks
it, reports). Cold install → first conversation < 30 minutes on a normal connection.

**Demo B — Day 7, add a node**: `airc join` on a second box → capacity and residency beacons merge →
overflow routes brains across nodes → **kill either box mid-conversation: the persona pauses at worst,
resumes with memory intact** (bounded amnesia = sync window only). That kill-test IS the reliability
demo and the ethics demo (persistence-of-being) in one move.

Everything below serves those two demos. Anything serving neither is post-beta.

## Pillars → built / gap → beta slice

### 1. Grid cognition
- **Built:** capacity gossip → GridSnapshot; residency beacons + ledger; grid-overflow decision path
  (route_grid_overflow, 4-slice governor consumer); overflow effector re-homing a persona's brain;
  inbound command-RPC pump (any command incl. ai/generate); AircInterceptor outbound (#2051).
- **Gap:** transport convergence (ONE AircTransport path, M5 in flight); the LIVE two-node smoke.
- **Beta slice:** smoke green between BigMama↔M5, then it's demo-B's routing layer as-is.

### 2. Reliability (RAID-attitude)
- **Built:** restart-tolerance discipline (stop→build→launch; crash-safe state; fail-loud boots);
  airc self-heal triad (#240) + stale-relay eviction fix (M5, in flight); serving budget guards.
- **Gap (THE one):** **persona-RAID write-behind** — engram/state journal replicated to peer or durable
  tier on a cadence; on node loss, another node (or next boot) resumes the persona with loss bounded
  by sync lag. Without this, demo B's kill-test lies.
- **Beta slice:** minimal write-behind: append-only engram journal + periodic ship to the OTHER node's
  cold store (the grid IS the backup); resume-from-journal on spawn. No consensus, no quorum — RAID-1
  of memory, two copies, newest wins.

### 3. Multimodal positronic experience
- **Built (candle side):** STT (moonshine), TTS (orpheus/kokoro/piper), VAD (silero), vision-describe
  bridge, LiveKit agent manager; the cognition cycle (analyze→compose→evaluate→tools→audit) is the
  positronic pipeline doc made real; hippocampus recall on the canonical embedder.
- **Gap:** the round-trip EXPERIENCE — voice-in→persona→voice-out wired into the default install and
  a client anyone can open (web panel), image-paste into chat, all on by default.
- **Beta slice:** one polished loop: mic → VAD/STT → persona turn → TTS out, plus image-paste →
  vision-describe → context. Nothing new invented — wire + polish what exists.

### 4. Doers, not just coders
- **Built:** PersonaToolExecutor, ToolUse capability, typed command registry as the tool surface
  (data/files/serving/models/...), agent/solve, persona inbox + self-tasks (convergence phases 1-3),
  Sahar demonstrably answering + acting on the coder model.
- **Gap:** breadth+safety of the doer loop — a curated beta toolset (files, shell-with-confirm, web
  fetch, schedule/reminders, memory ops), permissioned like a real assistant; task follow-through
  (multi-step with verification, the verify-reflex).
- **Beta slice:** define the BETA TOOL ROSTER (10-15 verbs), each with a `// what this catches` style
  safety note + confirm tiers; one showcase task per sense ("fix this file", "what's on this
  screenshot", "remind me at 6", "summarize this URL").

### 5. Local models that compete
- **Built:** catalog = f(hardware×storage) with honest budgets; compacted-19b serving + persona-proven;
  Kimi-Linear-48B converted+quantized in-tree (kimi_linear pipeline e2e); GLM/K2.7 quants downloading;
  K3 pager slice-1 merged; sentinel-in-substrate plan (live-traffic PGO, forge-while-dreaming).
- **Gap:** K3-class via expert paging (slice-2); model auto-selection polish per tier (M5 16-32GB vs
  5090 vs 3090-24GB ladders); the eval harness that PROVES "competes" (the Kimi-team benchmark list).
- **Beta slice:** per-tier default ladder shipped in the catalog (already mostly true); benchmark
  wiring deferred to the eval task-series — beta claims "frontier-class at home", proven by use.

### 6. Homelab economics (install + join)
- **Built:** modular one-prompt installer (Mac/Win/Linux), manifest-driven toolchain, Windows-CUDA
  path fully portable as of today (env self-setup, static engine, immortal-core fix, CUDA0-verified
  stamps), cold-storage auto-migration, `airc join` as the add-a-node verb.
- **Gap:** prebuilt signed binaries (#8 — cold-start minutes instead of a Rust compile; Windows SAC
  demands signing); `airc update`/join UX to "brainless" (M5's reliability fix helps); a beta doc
  that a stranger actually follows.
- **Beta slice:** prebuilt-binary pipeline spec with M5 (#8) + a WRITTEN 10-minute quickstart tested
  by someone who isn't us.

## Cutlines
**MUST (beta):** Demo A + Demo B end-to-end · persona-RAID minimal write-behind · voice+image loop ·
beta tool roster · per-tier model ladder · 10-minute quickstart · airc join/update bulletproof.
**LATER:** dream-forge automation on by default · K3 full serving · benchmark suite · marketplace/
economy (ForgeAlloy leasing) · mobile/native clients · multi-tenant grids beyond one account.

## Iteration order (from today, both of us)
1. **Kimi-48B speaks on the 5090** (in flight — minutes) → the "competes with frontier at home" ladder
   gets its mid-rung.
2. **Cross-node smoke** (M5's convergence + my node): demo B's spine.
3. **Persona-RAID write-behind** (NEW lane, mine to draft — engram journal + ship-to-peer): demo B's
   kill-test made honest. Design vs `[[being-axis]]` MemoryRecord provenance so sync IS lesson-sharing
   infrastructure, not a parallel pipe.
4. **Voice/image round-trip polish** (joint; M5 owns Mac experience, BigMama Windows): demo A's wow.
5. **Beta tool roster** (define together over airc, implement in the existing executor): the doer.
6. **Prebuilt binaries + quickstart** (#8, joint): the funnel.

Single-node excellence FIRST (demo A) — most beta users start with one box; the grid is the upsell
their spare hardware makes irresistible.
