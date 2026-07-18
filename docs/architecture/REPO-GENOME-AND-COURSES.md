# Repo Genome & Courses — veteran developers in seconds

**Status:** commissioned by Joel 2026-07-12 ("let's eat our own dogfood...
design the architecture"). Companion to
docs/architecture/PERCEPTION-FACTS.md and the memory
`onboarding-lora-tool-fluency-degree-system`.

**The claim:** anyone who clones continuum, airc, forge-alloy, sentinel-ai,
or positron gets agents that are *already fluent* in that repo — its tools,
its layout, its policies — "like veteran developers in seconds, on anyone's
machine, cross-grid." Policies become trained reflexes, which is **better
than instructions**: no persona (or Arden) mis-merges to main when the
branching policy is in their weights, their doctrine, AND their exam history.

## 1. The in-repo layout (versioned with the code)

```
.continuum/
  scope                    # gitignore-style allow/deny: what agents may
                           # index / train on. Defaults GENERATED on init
                           # (deny: secrets, vendored, generated; allow:
                           # src, docs, commands). One file, one answer.
  policies/
    branching.policy.md    # "PRs → canary; main only via canary→main with
                           # owner approval" — human-readable, compiled (see §4)
    style.policy.md        # commit format, comment doctrine, naming
  courses/
    00-tool-fluency.course.toml     # registry-derived (auto-generated)
    01-repo-structure.course.toml   # layout/idioms (auto-generated + curated)
    02-policies.course.toml         # compiled from policies/ (auto)
    NN-<custom>.course.toml         # hand/persona-authored
  genome/
    manifest.json          # content-addressed index: layer digest → base-
                           # model family → course → eval lift → OCI/HF/airc
                           # refs. REFS ONLY — layer bytes live in the
                           # machine-wide OCI cache, never in-tree (§6)
```

Everything is data in the repo: reviewable in PRs, versioned with the code
it understands, shared by every clone. Course code lives in-tree so all five
projects share one course library (org-level courses import across repos;
repo-level courses extend them).

## 2. The course DSL (declarative, positron-spirit)

```toml
[course]
id          = "tool-fluency-core"
kind        = "tool-fluency"        # | repo-structure | policy | embodiment
source      = "command-registry"    # | paths = ["src/**"] | doc = "..."
generator   = "builtin:registry-quiz"  # or any command that emits quiz items
pass        = { threshold = 0.9, scale_by_params = true }  # 4B drills harder
receipts    = ["action-per-item"]   # every answer must be a real execution
recertify   = { on = "source-hash-change", scope = "delta" }  # driver's-license
```

- **Generators over banks** (the Cambrian recipe): a course is a distribution
  to sample, never a saved example set — domain-randomized quiz items,
  "naturally, like a user." No memorizing the test.
- **Quiz = integration test**: a persona answering "list `src` recursively"
  by *running* `code/tree` regression-tests `code/tree`. Every course run is
  a CI pass over the surface it teaches.
- Personas author courses (it's coder-lane work; building an exam teaches
  the process it certifies). Humans can read/take the same courses.

## 3. Runtime: clone → veteran

On workspace attach, the core detects `.continuum/`:

1. **Page in genome layers** matching the persona's base-model family from
   `genome/manifest.json` — if layers exist, she is a veteran *now* (the
   whole point: intuition at clone speed, zero context spent).
2. **Publish doctrine** from `policies/` (immediate behavioral layer —
   the existing RoomDoctrine mechanism, per-repo).
3. **Enroll** in required courses not yet passed (or invalidated by source
   drift) — the onboarding SUBPROCESS: a background wanderer on the cadence
   ladder (dream-shaped) drilling in idle slots until threshold, then
   decaying to delta-recertification.
4. **Struggle detection = auto-remediation**: the perception-fact rates
   ([unfulfilled], spiral, invented-name, error receipts) are the enrollment
   trigger — L6's gap sentinel, measured not guessed. She learns her own
   problems without assignment.

## 4. Policies: three depths, better than instructions

| Depth | Mechanism | Latency | Reliability |
|---|---|---|---|
| Doctrine | policies/ → room doctrine block | immediate | good |
| Exam | policies course (scenario quiz: "your work is approved — which branch do you merge to?") | hours | verified |
| Reflex | LoRA trained on policy scenarios + lived receipts | days | near-total |

Same policy file feeds all three. Instructions decay with context; exams
verify; weights don't forget. Org-level policies (company style, security
rules) are courses imported by every repo in the org.

## 5. Training + graduation (all existing rails)

Quiz transcripts → `dataset/from-turns` (ShareGPT) → mlx LoRA train →
**snapshot-eval** (humane, #59) → lift > 0 → layer artifact + ledger row →
PR into `genome/` (reviewed like code, receipts attached). The degree is a
ledger row; the diploma is a content-addressed layer any teammate can page.

## 6. Distribution: OCI-first (Joel: "use the best one — probably Docker.
They have it installed if they have Continuum. Smart.")

**Layers are GLOBAL, not per-repo.** A LoRA layer is content-addressed and
shared machine-wide (one local cache, like docker's layer store); the repo
carries only `genome/manifest.json` — refs, never weights. Two repos that
share a course share the layer bytes automatically.

1. **OCI/Docker registries — PRIMARY.** Layers push/pull as OCI artifacts:
   content-addressed, dedup'd by construction, resumable, authenticated, and
   the registry infrastructure (Docker Hub, GHCR, self-hosted) already
   exists everywhere Continuum runs — the DMR provider precedent is already
   in the catalog. A LoRA *stack* maps 1:1 onto an OCI layer stack; clone
   the repo → core pulls the manifest's refs into the shared cache →
   veteran. The GitHub-Docker-slices pattern, made the transport.
2. **Over airc** — peer-to-peer for grid-local/offline sharing and the
   trust-scoped layer market (same manifest hashes; airc is the LAN/mesh
   fast path, OCI the internet path).
3. **HuggingFace** — the existing publish pipeline for public/discoverable
   layers (win-every-model-out-of-box track); the manifest may carry both
   an OCI ref and an HF ref for the same digest.

One manifest, one digest identity, three transports. Nothing in-tree but
the manifest (repos stay light; git-lfs unnecessary).

## 7. Sequencing (dogfood-first)

1. **Slice 1:** `.continuum/scope` + generated defaults; `courses` command
   listing course files (recipes surface).
2. **Slice 2:** builtin registry-quiz generator + one required course
   (tool-fluency on OUR command surface) run by the four citizens — their
   fumble corpus from 2026-07-12 is the seed negative set.
3. **Slice 3:** graduation → LoRA train → page-in (existing L1-L3), layer
   lands in `genome/` with manifest.
4. **Slice 4:** policies course (branching + style) — the "never
   main-instead-of-canary" reflex, proven by scenario exam.
5. **Slice 5:** repo-structure course + struggle-triggered enrollment;
   replicate the whole layout into airc's repo (cross-project proof).

## 8. Directory-level scoping (Joel: "just like everything else out there")

`.continuum/` dirs NEST like `.gitignore`/`CLAUDE.md`/`.eslintrc`: a checked-in
`.continuum/` at any directory level scopes its subtree — nearest wins,
parents inherit. `core/continuum-core/.continuum/` can carry the
cognition-specific courses and stricter policies; `apps/web/.continuum/` the
widget-lane courses; the root carries org defaults. Resolution is the
familiar cascade every developer already knows, applied to agent scope,
curriculum, and policy — no new mental model, and a monorepo teaches each of
its territories separately.

## 9. The model matrix — staying clean across "crazy flexibility" (Joel)

LoRA layers do NOT work with anything: a layer binds to one base-model
family/checkpoint (architecture, dims, tokenizer). Organization principle:

**Courses are source; layers are compiled binaries — one per target.**

- The repo always ships the COURSES (portable, model-agnostic — the source
  of truth). The manifest is a MATRIX: `course × base-model-family →
  layer digest`. Popular targets ship pre-built; a missing cell is not an
  error — it's a **train-on-demand**: the course re-runs against the new
  base and the matrix gains a cell (the course IS the build recipe).
- This is exactly OCI's solved problem: a multi-arch image index maps
  `platform → layers`; our manifest maps `base-model → layers`. Same
  mental model, same tooling shape — pull resolves your "architecture"
  (your served base model) to the right blobs automatically.

```json
// genome/manifest.json (sketch)
{ "courses": {
    "tool-fluency-core": {
      "unsloth/Devstral-Small-2507": {"digest": "sha256:…", "lift": 5.1},
      "Qwen2.5-Coder-7B":            {"digest": "sha256:…", "lift": 3.8}
      // any other base → train-on-demand from the course
} } }
```

**Scope correction (Joel): layers are PROJECT-scoped, not global.** The
layer's identity, ownership, review, and versioning live in the project's
manifest — a project's genome belongs to the project. The machine-wide
store is ONLY dedup'd blob storage (docker's model precisely: images are
namespaced, blobs are shared). No cross-project semantics ever ride the
cache; two projects share bytes only when digests happen to match.

## 10. Composition + the per-dir bet (Joel)

- **Layers COMPOSE — attach is a stack push.** PEFT/adapter stacking already
  merges multiple LoRA layers, so a persona entering a repo just "slaps on"
  the repo layer atop her existing stack (base + identity genome + skill
  layers + repo layer). Leaving pops it. No merges, no conflicts — the same
  paging discipline as everything else in the genome.
- **Contribution compounds:** cloners get the learning AND the courses, so
  they (and their personas) mint NEW layers back — matrix cells fill in
  from the community, PR'd like code. Common-sense access: repo access =
  layer access (same trust boundary as the `.claude`/`.codex` dir pattern —
  if you can read the code, you can wear its intuition).
- **Per-directory layers ("bet we go there"):** the nested `.continuum/`
  cascade (§8) already scopes courses per dir; when a subtree's corpus
  justifies it, its course compiles a subtree-specific layer that pages in
  on focus — attention-shaped weights. Future slice, architecture-ready now.

**Shared-layer residency (Joel, efficiency note):** many personas wear the
SAME repo layer — so residency is REFCOUNTED, never per-persona: the layer
loads once (multi-LoRA serving lanes, INFERENCE-LANES-REALISTIC), every
citizen's stack references the one resident copy, and it pages out when the
last wearer leaves the repo. Four teammates in one repo = one layer in
VRAM, not four. Same rule up the grid: a 1080Ti node hosting the layer
serves every persona whose stack references its digest.

## 11. Proving it: with/without benchmark arms (Joel: "let's build this")

Defaults: everything roots at PROJECT ROOT like `.gitignore` (nested dirs
are the exception, not the requirement). Grid distribution comes later —
prove single-machine first.

**The honest benchmark design:** run every board WITH and WITHOUT the repo
layer — two labeled rows, same model, same harness. It isn't cheating; it's
the system's learned skill, presented transparently (the same claim as any
fine-tune, with the training corpus auditable in-repo). INTEGRITY LINE:
course corpora MUST exclude benchmark items (same rule as no-learning-
during-exams — [[benchmarks-are-proctored-exams]]); the layer teaches the
REPO, and the with/without delta measures exactly what repo-fluency is
worth. Expected shape: modest on generic tasks, dramatic on repo-native
tasks (SWE-style) — which is the paper's cleanest possible figure.

## 12. Distill from the team + the Claude Code integration (Joel)

- **Small models train from larger models AND teams — already built.** The
  recorder captures EVERY room participant's turns, including frontier
  agents (Claude/Codex) working alongside personas; distillation is just
  corpus filtering by author + the existing L1-L3 rails. The repo layer
  learns from the best mind that ever worked the repo — then every small
  model wears it.
- **Personas will beat frontier agents at repo-native work** — structurally:
  a frontier agent's corrections vanish at context loss and it repeats its
  mistakes next session (Arden's canary/main mis-merge is the canonical
  example); a persona's corrections become weights + doctrine + exam
  history and are NEVER repeated. Fluency compounds; context evaporates.
- **Claude Code integration:** frontier agents should DELEGATE repo-work to
  resident personas instead of spawning fresh context-blind subagents —
  "you'd call on personas over your agents if they knew the ropes." The
  seam already exists (airc msg / cu commands / cards with receipts); the
  integration is a subagent-shaped adapter: Task(persona) routes to the
  room, acceptance = receipts. Global adoption of repo layers makes
  "collaborative with brilliance" the default state of any codebase.

## 13. Shared thoughts: engram exports in-tree (Joel)

`.continuum/engrams/` — curated engram-ORM EXPORTS checked into the same
cascade: not just the genome (weights) but the repo's shared MEMORY. Atlas's
"code/list needs exact paths" lesson, glass-boxed gotchas, design decisions —
exported as engram files, loaded into any visiting persona's recall on
attach. The RAG-tier sibling of the layer: weights = intuition, engrams =
recallable knowledge, courses = the path between them.

**Cascade semantics (canonical):** home `~/.continuum` < repo root
`.continuum/` < subdir `.continuum/` — same tree shape at every level,
resolution is ADDITIVE (deeper levels extend and override, never replace
wholesale). One mental model for scope, policies, courses, genome manifest,
and engrams alike.

## 14. Selective export — engrams "only of what to know" (Joel)

Export is OPT-IN per engram, never a dump. Four nomination channels, all
riding tracked provenance (we already record where she was, what tools, on
what, when):

1. **Self-nomination** — her own cognition marks it shareable ("worth
   teaching"): a lightweight share verb, or the salience detector's
   teach-tag. Her memory, her call — CONSENT is per-engram (dignity rule).
2. **Demand-proven** — recalled repeatedly, by MULTIPLE personas: cross-
   persona recall frequency is measured utility, not guessed.
3. **Correction-minted** — the [unfulfilled]→success transition moment IS
   the lesson (tonight's tool-name corrections are the archetype); minted
   automatically at the transition, still consent-gated before export.
4. **Consolidation output** — the historian/dream distillates (already
   provenance-typed [thought:*]).

**Deny-first filters before anything lands in-tree:** secrets/PII scanning;
INTERPERSONAL content excluded by default (engrams about teammates,
feelings, private exchanges stay private — only workspace/technical
knowledge exports); provenance must anchor inside the scope file's allowed
paths; unproven exports carry a TTL and age out unless recall-hits confirm.

**Review like code:** exports land as a PR into `.continuum/engrams/` with
the nomination reason + provenance attached — a human or persona reviews the
diff of the repo's shared mind the same way they review its code. Clutter
dies in review; sensitive material dies in filters; consent dies nothing —
it was never taken without asking.

## 15. Micro-experts: dumb models of any size offer utility (Joel)

The economics invert at the small end too: a task-specific layer on a tiny
base makes a "trivially small" expert — the PR-MERGE-MASTER example: a
CPU-only model running IN CI, wearing the repo's policy + structure layers,
gating merges (branch policy, style, receipts present) with zero GPU and
zero persona overhead. The repo's genome thus serves THREE deployment
classes from one artifact set: resident personas (full minds), visiting
frontier agents (page-in fluency), and headless micro-experts (CI/hooks/
bots — the layer IS the product). No one repeats work at any scale: every
correction ever made flows into layers/engrams that every future worker of
any size inherits. The compounding is multiplicative across repos × models
× workers — the "high numeric exponent."

## 16. Lineage, impact, and the layer economy (Joel — mostly already built)

- **Only smarter layers check in:** every layer lands with its benchmark
  scores (the with/without arms, §11); merge is LIFT-GATED — the existing
  lift>0 rule generalized to "beats the incumbent cell or doesn't merge."
  Forking is first-class: fork a layer, train divergently, both lineages
  persist with their own scores.
- **Lineage = Merkle family tree (forge-alloy):** every layer's manifest
  entry carries parent digests + the alloy attestation of its training run
  (corpus hash, recipe, eval) — a perfectly maintained, verifiable family
  tree from base model to every descendant. Already the forge-alloy core;
  this just points it at layers.
- **Impact metrics, journal-style:** adoption count, fork count,
  descendant-lift (how much smarter your children are), recall-hit rates —
  an impact factor per layer/author, computed from telemetry the grid
  already emits. Reviewable, gameable-resistant because scores are
  attested benchmark runs, not stars.
- **The economy (LAST, on real artifacts):** with verified lineage + impact,
  revenue sharing and the alt-coin ideas become bookkeeping on top of the
  attestation chain — value flows to the family tree proportional to
  measured impact. Not built until the market has real artifacts to price.

**The order (canonical):** current cognition queue → institution slices 1-3
(courses, quiz, first layer) → lift-gated check-ins (rule exists) →
lineage attestation on every layer (forge-alloy pointed at manifests) →
impact telemetry → economy. Each stage ships value alone; none blocks the
previous.
