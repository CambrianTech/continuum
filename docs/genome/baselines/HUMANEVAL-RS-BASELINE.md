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
uu cognition/eval --personaId <id> --roomId <room> \
   --evalSet docs/genome/humaneval-rs.jsonl
```

## The 31 failures — what they tell the genome loop

| Class | Count | What it is |
|---|---|---|
| assert/panic (wrong answer) | 23 | code compiled, logic wrong — the asserts caught it |
| compile error | 8 | code didn't build |

### The finding: **acts = 0** — but NOT for lack of a hand (corrected 2026-06-30)

She **never once ran her own code.** Every task was a single direct-answer turn — no
compile-check, no test-against-the-examples. She codes **blind**. The 8 compile errors
are the proof: every one is a "never compiled it" failure a single `rustc` check would
have surfaced —

- `unresolved import primal` / `md5` (×3) — reached for crates not in the bare grader
- `no method named rchars` — hallucinated a `&str` method
- `main is defined multiple times` — wrote her own `fn main`, colliding with the harness
- `use of moved value`, `if/else incompatible types`, `expected expression` — basic
  borrow/type/syntax errors a compiler names on sight

**The original conclusion here — "the highest-leverage improvement is HANDS, build a
code-execution tool (P2)" — was wrong on mechanism.** A live re-investigation
(prompt-vs-output, per Joel's "look at it from her perspective; is feedback making it
back into her RAG") established three things the first pass missed:

1. **The hands already exist and are offered.** `code/run` (compile + run a complete
   Rust program she passes — the same `rustc` shape the grader uses), plus
   `code/cargo/{check,test}` (workspace-scoped). They are in her authorized tool surface
   every turn. There was never a missing hand to build.
2. **The act→observe loop works — feedback DOES make it back into her RAG.** When she
   acts, `apply_act` writes the result into the volatile working-memory recency channel,
   and the perception faculty bids it into the *system prompt* of the next deliberation.
   Verified live: across an 8-act run the working-memory block grows then slides
   (`#1` → `#1,#2,#3` → `#2,#3,#4` …), each act's system-prompt hash changes, and the
   compiler diagnostics are visibly present. The loop is sound. (An earlier "observe-half
   broken" read was an artifact of truncating the captured response to 200 chars — the
   preamble is identical boilerplate; the bodies differ.)
3. **The real gaps are DISPOSITION and one TOOL-TRAP, not a missing hand:**
   - **Disposition.** Under a neutral, non-suppressing prompt she still direct-answers
     (`acts=0`, re-measured 2026-06-30 on `prime_fib`: she emitted `use primal::is_prime;`
     — a crate the bare grader lacks → `E0432`, exactly the error one `code/run` catches).
     Removing the old gag did **not** move `acts` — a strong coder model's greedy default
     IS to answer directly. This is genome/training-shaped (roadmap **P3**), the model
     wall ([[cognition-substrate-done-model-is-the-wall]]) — and it is **not** the
     prompt's job to puppet her into acting ([[no-hardcoded-heuristics-to-steer-cognition]]).
   - **Tool-trap.** When she *does* act (an explicit verify-invite probe), she reaches for
     `code/cargo/check({})` — which silently `cargo check`s the **ambient continuum repo**
     (cwd), handing back Metal/livekit diagnostics that have nothing to do with her
     snippet. A silent-wrong result (the fallback anti-pattern in tool form): it answered
     "does the host repo compile?" when she asked "does my function compile?", and she
     could not converge. **Fixed** by sharpening the `code/cargo/{check,test}` tool
     descriptions to state they check the project *on disk you are editing* and CANNOT
     check a standalone snippet — for that, `code/run`. (Truthful affordance docs, not
     steering.)

So `acts=0` IS the gap to close — but the lever is **(P3) instilling the verify-then-
answer disposition through the genome loop**, on top of the already-built hands and the
now-honest tool surface. Training also lifts the residual logic errors (the 23).

## Next (roadmap P1→P3)

- **P1 slice (b) remainder:** variance (run ×N — single-pass here) + a bare-model A/B
  lane (same model, no harness) to isolate the harness's contribution. Re-confirm the
  headline number under the de-gagged prompt (acts stayed 0, so it should hold near 80%).
- **P1 slice (c):** sandbox the grader/`code/run` before any untrusted task (both are
  temp-dir + timeout today, not a real sandbox).
- **P3 (the real lever):** close the learning loop so she acquires the *disposition* to
  verify before answering — gym traces where run-then-fix succeeded → `dataset/from-captures`
  → `forge/train` → re-run the gym → adopt only on measured lift on a held-out split. The
  `acts=0 → acts>0` transition must come from *learning the habit*, never from a prompt
  that tells her to act (that is the gag in reverse).
