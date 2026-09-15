# Why the serving loop is ours: the receipts behind the Rust substrate and the forked server

Joel, 2026-09-13: *"Prove Rust is why the Python engines are stuck. Or those who offload the
entirety to llama, which we've forked and streamlined."* This document is that proof, written
from measurements taken on the M5 that day, not from preference. Every number below has a probe
class or a command beside it so it can be re-taken.

## The claim, stated so it can be falsified

A team of local coding citizens needs three things a hosted inference engine cannot give it from
the outside:

1. **A register file per mind, not per request.** A citizen's warm state (KV cache, recurrent
   state, the checkpoints inside it) must survive being switched out for another mind and come
   back warm, on disk, across a reboot. That is a context switch, and it lives in the server's
   slot code, below any HTTP API.
2. **A scheduler that knows who speaks next.** Restore-ahead, slot affinity, eviction priced by
   the cost of the tail it would forfeit. That lives beside the cognition loop, not in an engine
   that sees requests as strangers.
3. **A prompt built to be reused.** The ledger of results, the working memory, the volatile tail
   must be laid out so the shared prefix is long and stable. That is the mind's composer, which an
   engine never sees.

An engine that is driven through its API from Python can do none of the three, because each one
is a change *inside* the serving loop or *inside* the mind. The receipts below are the three
changes, made in a day, each measured.

## 1. The register file: a restored slot was dead, and only the server could fix it

**What was wrong.** On a hybrid-cache model (Ornith 35B-A3B: GatedDeltaNet + attention) the
server refuses partial cache reuse (`cache_reuse is not supported by this context`) and can only
reuse a prefix by rolling back to a *context checkpoint* of the recurrent state. The slot save
file carried the KV and the tokens but not the checkpoints; a restore cleared them. So every
return from disk hit the reset path and re-prefilled from token zero.

| Measurement (`serving/cache-probe --roundtrip`, live M5, 2026-09-04 and 2026-09-13) | Result |
|---|---|
| Restore ok? (`n_restored`) | yes, 11,298 tokens, 48 ms |
| First request after the restore, installed server | `cache_n 0`, full prefill (4,342 ms on a 1,321-token probe; ~40 s on a citizen's 25k prompt) |
| First request after the restore, our fork with the checkpoint sidecar | `cache_n 1317` of 1321, 163 ms |
| On the live serve after the deploy | `restore_verdict: reuses`, `after_restore_cache_n 620/624` |

**The fix is 73 lines in `tools/server/server-context.cpp`** (CambrianTech/llama.cpp #2): the
checkpoints ride in a sidecar beside the slot state and come back with it. There is no API
parameter for this. A Python client of llama-server, vLLM, SGLang or Ollama cannot ask for it;
the state that matters is not exposed. Owning the fork is what made the register file real.

Two more server-side facts from the same day, each unreachable from a client:

- Checkpoint spacing defaults to 8,192 tokens. A citizen's prompt diverges from the previous
  turn at about 5k tokens (the results ledger), so the nearest checkpoint sat at ~350 tokens and
  every pinned turn rolled back to the head. `--ctx-checkpoints 32 --checkpoint-min-step 1024`
  puts one within a kilotoken of any divergence (#4007).
- A deferred restore was not bound to its slot (`pop_deferred_task` matched `task.id_slot == -1`),
  so a returning mind's restore starved behind whichever generation ended last. Fixed in the fork
  (0a637ba22), still unfixed upstream on 2026-09-13; the test that proves it is measured in
  timestamps because a fast runner ends a tiny model's generation before any flag can be read.

## 2. The scheduler: the cognition loop is the cache prediction

The substrate already had the paging engine (`PagedResourcePool`), typed activity keys
(`persona × room`), restore-ahead ("the scheduler's knowledge of who generates next IS the cache
prediction", cefc2ebfc) and the disk page store keyed by geometry. What broke it was arithmetic
at the seams, all of it in Rust, none of it visible to an engine:

| Defect (all measured 2026-09-13 on the live serve) | Where it lived | Fix |
|---|---|---|
| 5 minds on 4 warm slots: the slot directory reserved a scratch slot, the spawner capped the roster at lanes → 60 context switches on five activities in 40 min, every one a full re-prefill | `slots.rs`, `spawner_module.rs` | one `citizen_slots` rule; lanes never cap the roster (#4007) |
| The served window drifted every boot (83,968 → 102,656 → 126,464 → 138,240) and the page dir is keyed by it → no page ever survived a reboot | `serving_plan.rs`, `llama_server.rs` | the served window is sticky across runs (#4012) |
| The host-RAM prompt cache (`--cache-ram`, the fast tier for the most recent pages) floored at 256 MiB because the per-slot window ate the RAM | the same planner | window follows demand → 4,096 MiB on the same box after #4012 |

A scheduler that decides *who runs next and where their state lives* has to sit beside the loop
that decides *who thinks next*. That is the Rust substrate. An engine on the other side of a
socket can only be told "here is a prompt".

## 3. The prompt: divergence is a composer property

A checkpoint only helps if the prefix before the divergence is long. Across 55 consecutive
captured turns from eight personas (`~/.continuum/fixtures/prompt-captures`), the first block to
differ from the previous prompt was:

| First differing block | Count of 55 |
|---|---|
| the oldest surviving `[result #n]` (the ledger evicted one entry per settle) | 28 |
| a new room message | 5 |
| `[working-memory]` | 2 |

The ledger now collapses to half its cap in one jump so its head stays byte-identical across
several settles (#4008). The divergence moves to the volatile tail, which is where a checkpoint
can catch it. This is a change to how a mind writes its own prompt. No engine has that seam.

## What this buys, in the citizens' own turns

Before (a8687f0da, 52 generations, 2026-09-13 18:20–19:20Z):

| Measure | Value |
|---|---|
| Prefill per turn, median (p90) | 42.5 s (60 s) |
| Decode per turn, median | 153 s |
| Turn latency, median | 208 s |
| Tokens served from cache | 21k of 1.18M (1.8%) |

After (208bd91b7, 38 generations, 2026-09-13 22:33–23:18Z, **12 minds on 4 lanes** — a harder
shape than the before-run's 5 on 5; same probe classes):

| Measure | Value |
|---|---|
| Prefill per turn, median (p90) | 27.6 s (56 s) |
| Decode per turn, median | 84 s |
| Turn latency, median | 144 s (−31%) |
| Tokens served from cache | 112k of 722k (15.5%) |
| Reuse on a returning mind's turn (after a restore) | 0.86–0.89 (max 0.905); prefill 6–7 s instead of 40+ s |
| Slot pages: saves / restores / restores that came back cold | 30 / 16 / 0 |
| Evictions vs pinned turns | 63 vs 34 (three citizen slots for twelve minds) |

The median is dragged by first turns and evictions — twelve minds over three warm slots page
constantly. Two things move that: the window now follows the sent prompt (#4015: ~37k per slot
instead of 137k, so the same RAM holds more lanes and a multi-gigabyte prompt-cache tier), and the
served window sticks across boots (#4012) so the pages under it survive the next deploy. Both are
planner and page-store changes in Rust; neither touches the model or the API.

## Why "offload the entirety to llama" is not the alternative

Everything above *is* offloaded to llama.cpp for the arithmetic: attention, the recurrent state,
sampling, the KV layout. What the fork adds is the parts of the serving loop that are about *our*
citizens: which slot holds whom, what rides with a page, where checkpoints sit, that a restore is
bound to its slot. Those are small (the sidecar is 73 lines) and they are the difference between a
warm restore and a dead one. The upstreaming ledger (card a9ced625) lists which of them go back
upstream, which point at existing upstream PRs (#26004 solves the same restore problem; #27861 the
same expert cache), and which stay ours because they are Continuum-shaped.

The Python-driven path is stuck for a structural reason, not a language one: the seams that
decide latency here are below the API. Rust matters because the substrate that owns the mind's
loop, the paging engine and the scheduler runs in the same process as the composer and can hold a
slot pin across an await without a tick loop and a socket in between. It could be written in
another systems language; it could not be written as a client.
