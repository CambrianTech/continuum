# serving-replay

Slice S0 of [SERVING-ENGINE.md](../../docs/architecture/SERVING-ENGINE.md), plan line 3b.
It is the validator every later serving slice proves its gain on before that slice
reaches a live node.

It replays act sequences against an engine and reports per-act numbers in seconds,
with no residents and no core:

- reuse (cached tokens divided by prompt tokens), against the reuse the prompt made available;
- fresh prefill tokens;
- page-ins and their kind (`resident`, `restored`, `cold`, `restore_rejected`);
- evictions, with page-out and restore milliseconds;
- relaunches, which must be 0;
- wall time.

## The seam

```rust
pub trait ServingTarget {
    fn page_in(&mut self, mind: &MindId) -> Result<PageInReceipt, TargetError>;
    fn act(&mut self, mind: &MindId, request: &ActRequest) -> Result<ActOutcome, TargetError>;
    fn page_out(&mut self, mind: &MindId) -> Result<PageOutReceipt, TargetError>;
    fn stats(&mut self) -> Result<EngineStats, TargetError>;
}
```

The seam is keyed by mind, never by slot. Which seat a mind holds, and whom to displace
to seat her, is the engine's business (S5's `seq_table` and `scheduler`). Engines that
expose raw seats, today's llama-server and the test fake, sit behind `SeatedTarget`.
`SeatedTarget` is today's core policy written once: seat a mind in a free seat or
displace the least recent mind, save the displaced mind's page with
the live core's checkpoint subset (`keep_view_checkpoints`), and restore her page or start cold.
S1's RAM page verbs become a second `SlotEngine`, and S5's `continuum-serving` port
implements `ServingTarget` directly.

| File | Owns |
|---|---|
| `target.rs` | the seam: `ServingTarget`, `ActRequest`, `PageInKind`, receipts |
| `seats.rs`, `seated.rs` | the seat table and the paging flow for seat-exposing engines |
| `llama_server.rs` | llama-server: `/apply-template` (the engine's own template), `/completion` pinned to the slot with `cache_prompt`, `/slots/{id}?action=save\|restore` |
| `engine_process.rs` | an engine the replay launched: readiness, relaunch detection, and stop on drop |
| `http.rs` | blocking loopback HTTP/1.1 (std only; no async runtime or HTTP crate) |
| `acts.rs` | the act sequence and the available-reuse ceiling |
| `captures.rs` | sequences from real schema-4 prompt captures |
| `synthetic.rs` | deterministic N-mind interleavings with growing, append-only contexts |
| `replay.rs`, `report.rs`, `model.rs` | the driver, the JSON report and screen, and the model's arch and hybrid label |
| `fake.rs` | the scripted seat engine the tests drive (`test-fixtures`) |

## Run

**The engine must not run beside serving on a node with unified memory.** Run this on a
non-serving machine, or in a serving-down window: no `llama-server` on the live port and
no `continuum-core-server`.

```bash
export CARGO_TARGET_DIR=$HOME/.continuum/cache/cargo-target-loop
cargo test -p serving-replay                  # the replay's own logic, against the fake
cargo run -q --release -p serving-replay -- --synthetic             # 4 minds, 24 acts, 2 seats
cargo run -q --release -p serving-replay -- --captures ~/.continuum/fixtures/prompt-captures
```

The replay launches its own llama-server, on `--port` (default 58190, never the live port),
and stops it when the run ends. Pages go to a temporary directory that is removed
afterwards. The report is written to `--out`, or to the system temp directory.

The run exits non-zero when any act failed or the engine relaunched.

| Flag | Default | Meaning |
|---|---|---|
| `--model` | `$CONTINUUM_HOME/models/validators/qwen35-0.8b.gguf` | Qwen3.5-0.8B, arch `qwen35`: hybrid, the same arch as the live 27B |
| `--engine-bin` | the `bin/engine-*/llama-server` whose stamp matches `core/vendor/LLAMA_CPP_PIN` | the engine under test |
| `--attach host:port` | off | drive an engine someone else runs (relaunches then read as unobserved) |
| `--seats`, `--seat-ctx` | 2, 98304 | `--parallel` and per-seat context (`-c` = seats × seat-ctx) |
| `--kv-type`, `--cache-ram-mib` | q4_0, 9696 | the live lane's values (`ps` on the M5, 2026-10-23) |
| `--minds` | 4 | minds replayed (synthetic count, or the busiest N in captures) |
| `--acts`, `--head-chars`, `--base-chars`, `--tail-chars`, `--reply-chars`, `--switch`, `--seed` | 24, 8000, 24000, 2000, 600, 0.6, 1 | synthetic shape |
| `--acts-per-mind`, `--purpose`, `--order`, `--message-tail-chars` | 6, `cognition/act`, `time`, off | capture selection. `--purpose any` takes every call. `--order round-robin` keeps each mind's own order and has the minds take turns, so minds captured on different days contend for the seats. Tail chars scale each message down to its newest end |
| `--n-predict` | 8 | tokens generated per act |
| `-- <args>` | | extra engine flags, appended verbatim |

The other engine flags mirror the live lane: `--cache-reuse 256 --ctx-checkpoints 32
--ubatch-size 2048 --no-context-shift --jinja --flash-attn on`. `--checkpoint-min-step`
is derived by the core per lane; pass it after `--` to match a specific lane.

### What the numbers mean, and their limits

- **Available reuse** is the share of an act's characters that are unchanged since the
  same mind's previous act. It is the prompt-side ceiling, so a gap between it and the
  achieved reuse belongs to the engine or to paging. For captures it also shows how much
  the mind's own prompt layout gives away (SERVING-ENGINE.md §4, lever 1).
- Acts run one at a time. The report measures reuse, prefill and paging. Concurrency
  (batched decode, lever 3) is a later extension of the driver.
- The replay generates `--n-predict` tokens, not her real reply. The next captured
  prompt carries her real reply, so the tokens generated in the seat diverge from it.
  Achieved reuse can therefore fall below live reuse by up to the length of a reply.

## Baseline (llama-server, fork e7e1077b3, 2026-10-24)

Run on the M5 (64 GB unified) in a serving-down window at about 03:55Z. The engine was
`~/.continuum/bin/engine-b/llama-server`, stamp `e7e1077b3:metal`, which is `LLAMA_CPP_PIN`.
The model was Qwen3.5-0.8B, arch `qwen35`, which is **hybrid**. Each run used 2 seats with
98,304 tokens per seat, the live lane's flags, and `--n-predict 8`. The engine was ready in
770 ms.

**Synthetic: 4 minds, 24 acts, switch 0.6, seed 1.** The replay ran in 11.0 s (12 s with launch).

```
reuse    all 80.6%   warm 93.4% of an available 93.4%
prefill  35732 fresh of 183748 prompt tokens  (median 527 fresh per act)
page-in  resident=12 restored=8 cold=4   evictions 10
paging   restore median 13 ms  page-out median 48 ms (49378436 bytes)
acts     24 (0 failed)  act median 282 ms  p90 1516 ms   relaunches=0
```

On append-only contexts, today's paging (checkpointed save, restore) reaches the ceiling:
every restored act reused 92-95%, as resident acts did.

**Real captures: the 4 busiest minds, newest 6 `cognition/act` each, `--order round-robin`.**
That is 22 acts of 9k-73k tokens. The replay ran in 179 s (180 s with launch).

```
reuse    all 54.8%   warm 65.8% of an available 77.6%
prefill  336612 fresh of 744967 prompt tokens  (median 9281 fresh per act)
page-in  restored=18 cold=4   evictions 20
paging   restore median 30 ms  page-out median 43 ms (142409276 bytes)
acts     22 (0 failed)  act median 4805 ms  p90 19774 ms   relaunches=0
```

**The gap is S1's target.** Six restored acts reused **0** tokens even though part of the
prompt was unchanged since that mind's previous act:

| Act | Mind | Prompt tokens | Unchanged share |
|---|---|---|---|
| 4 | 342cde5d | 16,019 | 75% |
| 7 | f8a1c99a | 18,442 | 57% |
| 8 | 342cde5d | 15,735 | 87% |
| 10 | 2f50b223 | 51,512 | 18% |
| 11 | f8a1c99a | 9,281 | 84% |
| 21 | 2f50b223 | 36,301 | 24% |

Inference, not yet probed: a hybrid model cannot roll back a partial match. The page carries
only the newest 4 checkpoints, so when the prompt diverges before all of them, the engine
re-prefills from 0 (SERVING-ENGINE.md §0). The synthetic runs never diverge mid-history, so
they never hit this case. Where restores did reuse, the cost was 30 ms against 2-40 s of
re-prefill. Lever 1 (moving [actions] to the tail) attacks the same gap from the mind's side.
