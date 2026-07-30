# Beta: activities as the unit, a learned per-activity value, always measuring

**Status:** beta architecture locked 2026-07-29 (Joel). Companion to
[[SELF-CALIBRATION-PROPRIOCEPTION]] (the scoring/assignment mechanics) and
BETA-ACTUALIZATION.md (reconcile — this is the activity + learned-value facet).

## The unit is the ACTIVITY (a room / an experience / a benchmark class)
We code a FEW activities we know we need; they're defined by the benchmarks (the K3
chart classes), our chat + video-chat experiences, and the open-system asks. The
benchmarks are not a scoreboard — they're HOW we achieve many of these
([[benchmark-learning-flywheel]]). Beta activity set:

| Activity | What it is | Critical components (lose one ⇒ score collapses) |
|---|---|---|
| **Coding** (devoted coder) | DeepSWE/ProgramBench/FrontierSWE/SWE-Marathon/Kimi-Code, livecodebench-rs | solves-the-problem (quality/pass), interactivity (≥ floor tok/s) |
| **Agentic** | BrowseComp/MCP/OSWorld/Terminal-Bench, tool loops | tool-exec correctness, doesn't-loop (thrash detection) |
| **Multimodal / vision** | visual benchmarks; video-chat perception | vision describe/STT present |
| **Chat** (rooms) | text with personas | responsiveness, coherence |
| **Video chat** (rooms) | multi-persona live A/V; the 14 vs 3 vs 1 case | TTS, STT (lose either ⇒ experience worthless), latency, fps |
| **Hermes / open-system asks** | [[hermes-grid-node]], onboarding + tech-support ([[ai-onboarding-and-tech-support-vision]]) | the ask actually gets served |
| **Continuous learning** | training as an activity — LoRA from graded failures | training completes + improves the graded metric |
| **Dream / sentinels** | background PGO / consolidation / sentinel-AI ([[ethical-substrate-raid-personas]]) | runs without stealing the foreground experiences |

## The comprehensive per-activity score — and why NEAR-zero, never zero
Each activity has ONE comprehensive score, criticality-gated (a degraded load-bearing
component — lose TTS/STT in video chat, can't-solve in coding — collapses the whole
score, [[SELF-CALIBRATION-PROPRIOCEPTION]]). **Degradation scores NEAR-zero, not zero:
zero is a worthless signal** — a dead gradient the ML can't learn from and can't tell
apart from "not attempted." Near-zero preserves HOW bad + how recoverable, so the
learner still has a slope to climb. Every score is comprehensive (the whole experience)
with sub-scores that roll up.

## Always measuring = always creating training data
Every activity run — served experience, benchmark, chat turn, video session — is
MEASURED and becomes a graded training example. That's how we learn "the value of
everything" and how we know what each individual (persona × model/tier × node-state)
is capable of: because we've tested + used them before, and kept the graded record
([[being-axis-shareable-learning]]). No throwaway runs.

## The arc: measure → (levers) → dynamic ML → grid → mesh → (later) economy
Measure across our activity requirements + benchmark goals → assuming we have the
LEVERS (e.g. the GPU expert paging we're building, model-tier switching, TTS/STT
degrade, page-out concentration) → design the algorithms + ML that respond DYNAMICALLY
and ACROSS THE GRID, knowing the value of everything. That is a p2p mesh; the compute +
artifact ECONOMY ([[grid-as-network-intelligence-mandate]]) comes LATER.

## Beta simplification: FREE + egalitarian grid (avoid complexity)
For beta: **make it all free, and treat the grid the SAME whether it's my own subnet or
peers I joined** — LAN nodes and joined peers handled identically, egalitarian, no
pricing, no economy. Solve single-computer AND grid; treat grid as grid regardless of
who owns the nodes. The economy + differentiated peer trust is a post-beta layer; beta
deliberately collapses it to "all nodes equal, all free" to ship the learning loop first.
Aligns with [[airc-route-adapter-hierarchy]] (LAN-first) and [[docker-as-grid-substrate]].
