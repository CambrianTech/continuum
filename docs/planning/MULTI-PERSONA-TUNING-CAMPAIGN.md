# The multi-persona tuning campaign

Status: the program of work, 2026-10-30. Joel: "We are just gonna spend some time improving this dynamic
system to get it just right. Make sure it's well simulated, see how the context adapts to many personas in
the real system on all your machines independently, both in terms of its RAG and eliminating looping and
thrashing, without grid, just to optimize it for these radically different three use cases. Make sure we
have all the llama.cpp lane stuff, budgeting and airc fixes, all the latest. Simulations again and glass
box. Then go back and try again in the real systems. See how it acts and adapts, with the causal system not
screwing up, and keep going till we feel like it has good multi-persona throughput."

The design it executes: [HOW-A-MIND-GETS-WHAT-SHE-NEEDS](../architecture/HOW-A-MIND-GETS-WHAT-SHE-NEEDS.md)
(requirements, loops, scenarios), [THE-PROVING-GROUND](../architecture/THE-PROVING-GROUND.md) (simulation and
proof), [ONE-BUDGET](../architecture/ONE-BUDGET.md), MIND-CONTEXT's causality section and render receipt,
[SERVING-ENGINE](../architecture/SERVING-ENGINE.md) S2.

## The three use cases (no grid yet)

Each machine runs on its own, so each is a different world for the same algorithm:

| Node | World | Residents | Owner of its rounds |
|---|---|---|---|
| M5 | unified memory (64 GB), Metal, Qwen3.6-35B-A3B, four fixed slots today | Aiko, Aris, Atlas, Benchy | Fable |
| 5090 | discrete CUDA, Windows | Kimi and the 5090's residents | Codex (with BigMama's measurements) |
| IntelMac | the low-end CPU floor: i7-8850H, 32 GB RAM; Qwen2.5-Coder-1.5B, two slots. Its GPUs lose to its CPU (measured below) | none until its core returns (stopped since 2026-10-30 00:52Z for incident containment) | Cormac (his node, his lifecycle) |

IntelMac's GPUs, measured by Cormac 2026-10-31 01:05Z (fork 08d6d354b scratch builds; Qwen2.5-Coder-1.5B,
an 840-token prompt, 64 decode tokens, cache off, 3 runs each):
- CPU: prefill 165-176 tok/s, decode 22.5-22.8 tok/s.
- Radeon Pro 560X (Metal 2, the system-default Metal device): the macOS AMD driver never returns from its
  first buffer allocation (`IOAccelResourceCreate` under `newCommandQueue`). This happens at backend
  registration, so even `--n-gpu-layers 0` hangs, and the process is left unkillable.
- Intel UHD 630 (Metal 3, unified, 1,536 MB working set): at a 512-token ubatch the macOS GPU watchdog kills
  the command buffer. At 64 it runs: prefill 12.3 tok/s (14x slower), decode 7.6 tok/s (3x slower).

So the engine's `metal` feature stays off on this node; its rounds work the CPU path. A device is used only
after an out-of-process probe passes and offload measures faster (THE-PROVING-GROUND tiers).

Receipts: IntelMac read by Cormac 2026-10-30 ~22:40Z; its core needs a canary build with #5603's addressed
ingress before its first round. Placement across nodes waits until all three
pass on their own.

## Phases

**P0. One "latest" on every node.**
- Triage the open PRs in this system's path (each merged, reworked to the design, or closed with a reason;
  none left hanging): #5602, #5591, #5576, #5558, #5527, #5518, #5485, #5529, #5604, #5609, #5611, #5612,
  #5613, and whichever others the triage finds.
- Every node on the same canary, the same fork pin with the current lane work, and the same airc release.
  Each node's running revisions are recorded as a receipt (core, engine, airc, model, slot geometry,
  residents), not assumed.

**P1. The proving ground, T0** (THE-PROVING-GROUND section 8, steps 1 to 3):
- one node world model, calibrated per node from its own measurements;
- production policy driven through its seams;
- the known-bad suite, which must fail before anything else is trusted.

**P2. Each fix through the prover, then live.** In order:
1. the think-cut interim (Cormac's option C: a cut inside thinking grows her grant; the fixed 1,024 think
   budget deleted);
2. causal items (#5611), the render receipt, and the stops counter;
3. the window (#5612);
4. the open-loop rules replaced one at a time by L1 to L3 (HOW-A-MIND section 5).

Every change carries a written T0 prediction for each node before it deploys.

**P3. Rounds on each node, in parallel.** A round:
1. predict (T0/T1, that node's profile);
2. deploy (a coordinated serving-down window on that node);
3. glass-box the residents for 15 minutes and an hour (the acts inspector, the health line, the receipts);
4. compare with the prediction;
5. build what live taught into the simulation first (Joel: "Whatever we learn from real life we try to then
   build into simulations to try to correct"): a new scenario file, a known-bad, or a world-model correction,
   until the simulation reproduces what live did;
6. fix the largest gap, as a change to the design, proven in the corrected simulation, never a one-off
   patch;
7. repeat.

**Design in place for everyone, then fix for one another, merging as we go** (Joel). The designs merge to
canary first so every agent works from the same text; after that, each agent fixes what blocks another,
and work merges as it is proven, never parked.

A round's finding that applies to all nodes is proven on one and carried to the others.

**P4. The engine side,** once the mind side holds: SERVING-ENGINE S1 to S3 (unified state, live
reconfigure, paging) and L5's lane price, through T2 first, then the same rounds.

## Gates

Product goals, not machine constants (CLAUDE.md plan, goal 2), plus the design's invariants:

| Gate | Per | Done when |
|---|---|---|
| Tool calls | resident-hour | 40 or more |
| Median act | resident | under 60 s |
| Prefix reuse | resident | 80% or more |
| Merged PRs | node-day | at least one resident PR |
| Repeats (unchanged re-read of a whole item) | resident-hour | none twice in an epoch (S7) |
| Cuts | resident | no identical cut twice |
| Reversals | loop | counted, and no scenario fails on it |
| Relaunches while serving | node | zero |
| Known-bad suite | every change | all known-bads still fail in the prover |

The campaign ends when each of the three nodes holds these over a full day of the residents' own work, and
the prover predicted that day within its published error.

## Coordination

- Ownership follows the designs: Fable has the mind side (L1 to L3, the receipt, the engine verbs) and the
  M5's rounds; Codex has admission, residency, the node loop and the 5090's rounds; Cormac has the outcome signals, the
  behaviour model, the scenario and known-bad suites, the reductions and IntelMac's rounds.
- Each round's prediction, receipts and finding go on the round's card, not into this document.
- No round deploys a change that did not pass T0 against the known-bad suite.
