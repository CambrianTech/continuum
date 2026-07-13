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
    manifest.json          # content-addressed index: layer → base-model
                           # family → course → eval lift → OCI/HF ref
    <family>/<layer>.gguf-lora      # small layers in-tree (git-lfs);
                                    # big ones by manifest ref only
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

## 6. Distribution (in order of preference)

1. **With the repo** — small layers in-tree via git-lfs; the clone IS the
   install. The repo becomes its own model card.
2. **OCI/Docker layers** — content-addressed, dedup'd, registries everywhere;
   `genome/manifest.json` carries the ref (the GitHub-Docker-slices pattern;
   literal docker registries as genome CDN if nothing better exists).
3. **Over airc** — genome events peer-to-peer for grid-local sharing
   (the layer market, trust-scoped).
4. **HuggingFace** — the existing publish pipeline for public layers
   (win-every-model-out-of-box track).

One manifest, four transports; the manifest hash is the identity everywhere.

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
