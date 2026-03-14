# Recipe = Sentinel Template = The Universal Workflow Engine

> The recipe defines the workflow. The sentinel runs it. The personas fill the roles. Everything else is already built.

## The Convergence

A **recipe** and a **sentinel template** are the same thing — a workflow shape encoded as a pipeline DAG. The recipe catalog IS the template catalog. When a persona learns a new workflow, they're learning a new recipe, which is a new sentinel template.

The sentinel pipeline engine has 10 step types: Shell, LLM, CodingAgent, Command, Condition, Loop, Parallel, Emit, Watch, Sentinel. It doesn't know or care if it's merging git branches, arranging song stems, or editing a manuscript. It executes the DAG.

**A recipe is also the collaborative integration layer.** It defines how multiple agents combine their work. Without a recipe, you get chaos — everyone edits the same file, voice inconsistency, duplicate work. With a recipe, you get a production pipeline.

## The Generalized "Gitflow"

Every collaborative workflow follows the same fundamental shape, regardless of artifact type:

```
Branch → Isolate → Work → Validate → Integrate → Verify
```

What changes per task type is what each of those steps MEANS:

| Step | Code | Novel | Song | Research |
|------|------|-------|------|----------|
| **Branch** | git branch | chapter outline | concept/brief | research question |
| **Isolate** | git worktree | draft in separate file | separate stems/tracks | independent investigation |
| **Work** | code + build | write prose | compose/record | gather findings |
| **Validate** | unit test, type check | self-review, spell check | listen-through, timing | source verification |
| **Integrate** | git merge | revise into manuscript | arrange + mix | deduplicate + synthesize |
| **Verify** | integration test | editorial review | master + A/B test | peer review + cite check |

Git happens to be the branching/isolation/integration mechanism for code. For a novel, it's chapters in separate files. For a song, it's separate stems. For research, it's separate findings documents. The sentinel pipeline is the universal executor.

## Four Fundamental Workflow Patterns

### 1. Divide-and-Merge (Code Development)

The artifact is modular — you can split at file/module boundaries and merge mechanically.

```
Persona A ──→ worktree/branch A ──→ code + test ──┐
Persona B ──→ worktree/branch B ──→ code + test ──┤──→ sequential merge ──→ integration test ──→ main
Persona C ──→ worktree/branch C ──→ code + test ──┘    (CodingAgent resolves conflicts)
```

**Merge operation:** `git merge` — mechanical, with CodingAgent for conflict resolution
**Validation:** Binary — build passes or fails, tests pass or fail
**Primary constraint:** API compatibility between modules
**Templates:** `dev/build-feature`, `dev/fix-bug`, `dev/integrate`

### 2. Primary-Author-With-Support (Creative Writing)

The artifact requires voice coherence — split authorship destroys it. ONE persona holds the voice. Others support.

```
Writer ──────────────→ draft chapter ──→ revise ──→ next chapter (LOOP)
                           ↑                ↑
Character Advocates ───────┤ (react, don't write)
Structural Editor ─────────┤
Continuity Tracker ────────┤
Line Editor ───────────────┘ (polish pass after revision)
```

**Roles:**
- **Writer** — primary voice, non-negotiable single author
- **Worldbuilding Researcher** — builds reference bible (places, rules, history, timeline)
- **Character Advocate** — one per major character. "Would Sarah actually say this?" Consistency police.
- **Structural Editor** — chapter-level arc, pacing, tension payoff
- **Line Editor** — prose quality, showing vs telling, word repetition
- **Continuity Tracker** — "In chapter 2 the car was blue. In chapter 11 it's red."
- **Beta Reader Personas** — different reader types (genre fan, literary critic, casual reader)

**Merge operation:** Creative revision — writer reads feedback and rewrites. Not a mechanical merge.
**Validation:** Qualitative — LLM judgment on coherence, voice, arc
**Primary constraint:** Voice consistency across the entire work
**Outer loop:** Full manuscript → developmental edit → revision → copy edit → proofread

### 3. Gather-and-Synthesize (Research / Documents)

The gathering phase is parallelizable. The synthesis phase requires coherent voice.

```
Researcher A ──→ investigate aspect 1 ──┐
Researcher B ──→ investigate aspect 2 ──┤──→ deduplicate ──→ synthesize ──→ cite-check ──→ final doc
Researcher C ──→ investigate aspect 3 ──┘
```

**Merge operation:** Deduplication + contradiction resolution + synthesis
**Validation:** Source verification, factual accuracy, completeness
**Primary constraint:** Factual correctness, no contradictions

### 4. DAG Pipeline (Multi-Artifact Production)

Some branches are parallel, some sequential, some independent. The "merge" varies by step.

**Example — Song/Album Production:**
```
Parallel:
  Branch A: Lyricist ──→ lyrics
  Branch B: Composer ──→ melody/chords
  Branch C: Artist ──→ album artwork (independent)
      ↓              ↓
Arrange: combine lyrics + melody into structured song
      ↓
Sequential: Produce → Mix → Master
      ↓                         ↓
Package: audio + art + metadata → Submit to iTunes/Spotify
```

**Example — Product Launch:**
```
Parallel:
  Branch A: Engineers ──→ build product
  Branch B: Marketing ──→ create campaign
  Branch C: Legal ──→ compliance review
      ↓
Integration: product + campaign + legal approval → launch
```

**Merge operation:** Varies per step — creative synthesis, mechanical packaging, approval gates
**Validation:** Varies per step — technical quality, brand consistency, legal compliance
**Templates:** `production/*` category

## Recipe as Integration Layer

A recipe defines the FULL collaboration shape:

| Dimension | What the recipe specifies |
|-----------|--------------------------|
| **Who** | Which personas participate, what roles they fill |
| **How** | Parallel, sequential, or DAG coordination |
| **Merge** | How individual work combines (git merge, revision, synthesis, packaging) |
| **Validate** | How to verify quality at each stage (build+test, review, fact-check) |
| **Escalate** | When to involve humans or smarter AIs (conflicts, quality gates, failures) |
| **Iterate** | Loop structure — per-chapter, per-branch, retry-on-failure |

Without this, personas are individuals doing separate things. With this, they're a team executing a production pipeline.

## Implementation

All four patterns map to the same sentinel pipeline primitives:

| Pattern Need | Pipeline Primitive |
|---|---|
| Parallel work | `parallel` step with branches |
| Sequential steps | Linear step ordering |
| Iteration | `loop` step (count, while, until) |
| Feedback/review | `emit` → `watch` (post to chat, wait for response) |
| Conditional logic | `condition` step (if/then/else) |
| AI judgment | `llm` step (review, synthesize, evaluate) |
| Coding/editing | `codingagent` step (autonomous file manipulation) |
| System interaction | `command` step (chat, data, tools) |
| Sub-workflows | `sentinel` step (nested pipeline) |
| Coordination | `emit`/`watch` between sentinels |

The tools are all there. The recipe is just the wiring diagram.

## Template Categories

```
dev/*          — Code development (divide-and-merge)
creative/*     — Creative writing (primary-author-with-support)
research/*     — Research and analysis (gather-and-synthesize)
production/*   — Multi-artifact production (DAG pipeline)
academy/*      — Training and education (teacher-student sentinel pairs)
eval/*         — Evaluation and benchmarking
```

Each category encodes its collaboration pattern. New categories emerge as new workflow shapes are discovered. Personas can create and share recipes — extending the template registry at runtime.

## The Vision

A persona receives a request. The request might be "build auth middleware" or "write chapter 7" or "produce a demo track" or "research competitor landscape." The persona:

1. Recognizes the task type
2. Selects the appropriate recipe/template
3. Fills in the parameters from context
4. Spawns the sentinel
5. Fills their role in the workflow (writer, reviewer, researcher, etc.)
6. Other personas fill their roles (character advocate, editor, etc.)
7. The pipeline orchestrates the collaboration
8. The result is a finished artifact — committed code, polished chapter, mastered track, research report

The persona doesn't think about git or file management or coordination. The recipe handles all of that. The persona thinks about the creative work.

## Role-Capability Matching

A recipe doesn't just define workflow steps — it defines **roles with capability requirements**. Like a manager staffing a project, the recipe declares what each role needs:

```
Role: "Audio Reviewer"
  Requires: audio perception (native or bridged via STT)
  Task: evaluate mix quality, timing, mastering

Role: "Art Director"
  Requires: vision (native or bridged via vision description)
  Task: evaluate album artwork, brand consistency

Role: "Code Reviewer"
  Requires: code comprehension, tool calling
  Task: architecture review, security audit
```

The sensory architecture already bridges capability gaps — text-only models get vision descriptions, non-audio models get transcriptions. But the recipe should declare requirements so the system can:

1. **Assign the best-fit persona** — native capability preferred over bridged
2. **Ensure the bridge is active** — if bridging is needed, verify the pipeline is configured
3. **Warn on mismatch** — "this recipe needs audio judgment but no audio-capable model is available"

This is the same as a teacher reviewing student work — a teacher CAN review a novel, a song, artwork, code, anything within their base model's capabilities. The recipe just needs to declare what those capabilities are so the right teacher gets assigned. A manager does the same thing: staff the project with the right people for the right roles.

**No faking.** If a role requires audio judgment and no audio-capable model is available, the recipe FAILS — it does not silently assign a text-only model that pretends to hear the mix. A disabled or faked persona/sentinel is worse than no persona at all. You'd rather know the capability is missing than get confident nonsense. The recipe must either have the real capability or refuse to run.

**Organizational vs. perceptual roles:** A manager/coordinator doesn't need to hear the mix — it needs to know whether the audio reviewer approved it. Management is organizational (planning, assignments, quality gates, coordination), not perceptual. The recipe separates these:

- **Organizational roles** (manager, coordinator, scheduler) — need reasoning/planning. Any capable LLM can fill these.
- **Perceptual roles** (audio reviewer, art director, code reviewer) — need the actual sense. Must have native capability or a proven bridge.
- **Creative roles** (writer, composer, artist) — need both the sense AND the generative capability in that modality.

A persona that can't listen can't develop music. But it CAN manage the music production pipeline. The recipe declares which roles need which capabilities, and the system enforces it.
