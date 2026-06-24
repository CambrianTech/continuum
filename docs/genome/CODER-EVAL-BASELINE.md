# Coder-Eval Baseline — Asha (2026-06-23)

First objective measurement of a persona's coding ability on the **live** continuum
system (real `WorkspaceCycle`, real tools, real model), via the `cognition/eval` gym.

- **Persona:** Asha `90e758b2-3cf3-45c1-b100-de7c4ab5a549`
- **Model:** `qwen3.5-4b-code-forged` (served on llama-server :58057)
- **Grader:** `cognition/eval.rs` — extracts the code block, wraps it as
  `{code}\n\nfn main(){{ {test} }}`, compiles with `rustc --edition 2021`, runs the
  binary. PASS = exit 0. Objective, diagnosable (compile errors + panic locations),
  Rust (not substring-on-prose, not Python).

## The two corpora (distinct concerns)

| Set | File | Measures |
|---|---|---|
| Navigation / knowledge | `coder-eval.jsonl` (13) | Can she find & read the real repo and answer accurately? (substring-graded) |
| Code writing | `coder-write-eval.jsonl` (9) | Can she write a function that **compiles and passes a test**? (`rustc`-graded) |

The write set spans a deliberate difficulty ladder (arithmetic → strings → recursion →
vec ops → struct+impl → parsing → predicate → hashmap → generics) so failures localize
to a *kind* of code. All 9 tasks were validated with reference solutions (9/9 compile+pass
through the identical `rustc` wrapping) **before** grading Asha — so a failure is hers,
not the harness's.

## Results

| Run | Score | Notes |
|---|---|---|
| Navigation | **12/13 (92%)** | one miss: `why_focused_query` — found+read the file (acts=1) but gave the *what*, not the *why* (noise dilutes relevance). Reasoning depth. |
| Code writing (one-shot) | **7/9 (78%)** | `acts=0` on all 9 — she never self-verified. Misses: `is_palindrome` (dropped the `to_lowercase()` the prompt required → runtime panic), `max_item_generic` (`items.iter().max()` needs `Ord`; bound is `PartialOrd` → E0277). |
| Iterate experiment (the 2 misses, prompt invites `code/run` verify) | **0/2** | `is_palindrome`: `acts=0` again — *said* "Let me use the `code/run` tool…" then emitted code **without calling it** (narrate-not-act). `max_item`: `acts=2` — the loop **engaged**, she compiled, observed, reasoned the `Ord`→`PartialOrd` fix correctly — but her terminal turn was *"let me fix this and run again"* **prose with no final code**, so the grader compiled the prose. |

## Diagnosis

**Substrate: healthy and proven.** Tools fire, workspace layout is synced (every path
correct), the gym grades precisely, and — critically — when she *acts*, the act→observe
loop does exactly its job: `code/run` compiled her `max_item` attempts, folded the result
back, she re-perceived and re-reasoned. `code/run` is the right-shaped hand (standalone
`rustc` snippet, AiSafe persona tool). None of the failures are substrate.

**The wall is model reliability on the act-vs-narrate Decision.** Across all runs the 4B
model:
- one-shots instead of verifying (it *can* one-shot 78% of small functions, but won't self-check),
- narrates intent to act (`Speak` "let me use the tool") instead of emitting `Act`,
- ends mid-iteration with prose ("let me run again") instead of the final code.

Plus a thinner **Rust-idiom gap** (the `PartialOrd`/`Ord` `.max()` trap) — which she
*did* reason through once she was actually in the loop.

## The lever

This is **training-shaped, not knob-shaped.** Per `[[no-hardcoded-heuristics-to-steer-cognition]]`
we do **not** intercept her prose to force an `Act` (that's the puppeting anti-pattern).
The fix is the **genome loop**: this gym is now an objective fitness signal — generate
training traces of correct write→`code/run`→fix iterations (recorder → `dataset/from-turns`
→ `forge/train` → LoRA → genome page-in), targeting (a) emitting `Act` to self-verify by
default and (b) Rust idioms. Re-run the gym to score the delta. That is the `[[coordination-learning-flywheel]]`
made concrete: the gym measures, the genome closes the gap, the gym re-measures.
