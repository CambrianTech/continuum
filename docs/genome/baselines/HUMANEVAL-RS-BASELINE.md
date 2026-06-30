# HumanEval-rs Baseline — the P1 number

> **The gate number P1 has been waiting on.** Until a reproducible, test-graded
> baseline pass-rate exists, every later phase of
> [the roadmap](../../cognition/ROADMAP-TO-CODING-ITSELF.md) is a hypothesis. This is
> that number. (Roadmap P1, slice (b): first persona baseline.)

## The number

| Field | Value |
|---|---|
| **Pass-rate** | **80.1% (125/156)** |
| Benchmark | HumanEval-rs (`docs/genome/humaneval-rs.jsonl`, MultiPL-E Rust translation) |
| Persona | Solenne (`0d3209a1-…`), measured on her **live cognition** (snapshot-safe, #59) |
| Serving model | `qwen2.5-coder-14b-instruct` Q4_K_M (llama-server lane) |
| Date | 2026-06-30 |
| Grader | `cognition/gym_grader::test_grade` — compiles her Rust with `rustc --edition 2021` and runs the task's asserts; pass = exit 0 |
| Mean latency | 6.86 s/task (p95 11.4 s) · 28.6 decode tok/s · 0.68 cache-hit |
| Raw artifact | `humaneval-rs__solenne__qwen2.5-coder-14b-q4km__2026-06-30.json` (this dir) |

**Calibration check:** 80% sits squarely in the band MultiPL-E publishes for
qwen2.5-coder-14B-class models on Rust HumanEval — so the gym is measuring the right
thing, not a rigged number. This is an honest, comparable, reproducible baseline.

**Reproduce:**
```
cu cognition/eval --personaId <id> --roomId <room> \
   --evalSet docs/genome/humaneval-rs.jsonl
```

## The 31 failures — what they tell the genome loop

| Class | Count | What it is |
|---|---|---|
| assert/panic (wrong answer) | 23 | code compiled, logic wrong — the asserts caught it |
| compile error | 8 | code didn't build |

### The screaming finding: **acts = 0 on all 156 tasks**

She **never once ran her own code.** Every task was a single direct-answer turn — no
tool use, no compile-check, no test-against-the-examples. She codes **blind**
([[persona-codes-blind-no-hands]]). The 8 compile errors are the proof: every one is a
"never compiled it" failure that a single `rustc` check would have surfaced —

- `unresolved import primal` / `md5` (×3) — reached for crates not in the bare grader
- `no method named rchars` — hallucinated a `&str` method
- `main is defined multiple times` — wrote her own `fn main`, colliding with the harness
- `use of moved value`, `if/else incompatible types`, `expected expression` — basic
  borrow/type/syntax errors a compiler names on sight

**The highest-leverage improvement is hands, not weights.** A sandboxed
code-execution tool (compile + run the visible examples, re-perceive the result,
revise) would let her self-correct most of the 8 compile errors outright and a share
of the 23 wrong-answers — *without any training*. That is the acting-organism thesis
([[persona-codes-blind-no-hands]], roadmap P2): the gym rewards acting over narrating,
and `acts=0` is exactly the gap to close. Training (the genome loop, P3) lifts the
residual logic errors on top of that.

## Next (roadmap P1→P3)

- **P1 slice (b) remainder:** variance (run ×N — single-pass here) + a bare-model A/B
  lane (same model, no harness) to isolate the harness's contribution.
- **P1 slice (c):** sandbox the grader before any untrusted task (`test_grade` is
  temp-dir + 10 s timeout today, not a real sandbox).
- **P2:** give her the code-execution hand and re-measure — the `acts=0 → acts>0`
  transition should move the number with zero training.
- **P3:** close the learning loop on the residual logic failures (the 23), adopt only
  on measured lift on a held-out split.
