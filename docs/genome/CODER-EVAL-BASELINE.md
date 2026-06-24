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
| Code writing | `coder-write-eval.jsonl` (30) | Can she write a function that **compiles and passes a test**? (`rustc`-graded) |

The write set spans a deliberate breadth ladder (arithmetic → strings → recursion →
vec ops → struct+impl → parsing → predicate → hashmap → generics → encoding → sorting)
so failures localize to a *kind* of code. All 30 tasks were validated with reference
solutions (30/30 compile+pass through the identical `rustc` wrapping) **before** grading
Asha — so a failure is hers, not the harness's. (The set grew 9→30 to make the signal
robust enough to train against per `[[ask-anything-assemble-best-self-or-train]]`.)

## Results

| Run | Score | Notes |
|---|---|---|
| Navigation | **12/13 (92%)** | one miss: `why_focused_query` — found+read the file (acts=1) but gave the *what*, not the *why* (noise dilutes relevance). Reasoning depth. |
| Code writing — 9-task (one-shot) | **7/9 (78%)** | first cut; superseded by the 30-task run below. |
| **Code writing — 30-task robust (`max_acts=8`)** | **20/30 (67%)** | The robust baseline. `acts=0` on **29 of 30** despite `max_acts=8` — she one-shots everything and never self-verifies. 67% is what a 4B coder one-shots cold. |

### 30-task failures (the 10 the genome loop must close)

| Task | Class | Root cause |
|---|---|---|
| `dedup_sorted` | runtime panic | logic bug — a `code/run` against the example would catch it |
| `is_palindrome` | runtime panic | dropped the case/alnum normalization again → wrong on `"A man a plan…"` |
| `rotate_left` | runtime panic | off-by / wrap logic bug — verifiable |
| `title_case` | runtime panic | word-boundary/capitalization logic bug — verifiable |
| `max_item_generic` | E0277 | `T: Ord` needed by `.max()`; bound is `PartialOrd` (the recurring Ord trap) |
| `median` | E0277 | `f64: Ord` — sorting floats needs `partial_cmp`, not `.sort()` |
| `to_binary` | E0384 | forgot `mut` on the param before reassigning `n` |
| `count_vowels` | E0277 | `&char: Pattern` — passed `&char` to a `str` method expecting a pattern |
| `run_length_encode` | E0308 | return/collection type mismatch |
| `most_common_char` | E0308 | return type mismatch |

Two bands: **4 runtime panics** (logic bugs that one `code/run` self-check would have
caught) and **6 compile errors** in a tight idiom cluster — ordering trait bounds
(`Ord` vs `PartialOrd`, float sorting), `mut` params, str-pattern types, return-type
mismatches.

## Diagnosis

**Substrate: healthy and proven.** Tools fire, workspace layout is synced (every path
correct), the gym grades precisely, and — critically — when she *acts*, the act→observe
loop does exactly its job: `code/run` compiled her `max_item` attempts, folded the result
back, she re-perceived and re-reasoned. `code/run` is the right-shaped hand (standalone
`rustc` snippet, AiSafe persona tool). None of the failures are substrate.

**The wall is model reliability — two gaps, now measured robustly over 30 tasks.**

1. **She does not self-verify.** `acts=0` on **29 of 30** tasks even with `max_acts=8`
   explicitly inviting iteration. She one-shots, full stop. This is the single highest-
   leverage gap: 4 of the 10 failures are runtime panics (`dedup_sorted`, `is_palindrome`,
   `rotate_left`, `title_case`) that *one* `code/run` against the prompt's own example
   would have surfaced — free wins left on the table because she never runs her own code.
   The earlier 9-task "iterate experiment" showed the failure shapes when she's pushed to
   act: narrates intent (`Speak` "let me use the tool") instead of emitting `Act`, or ends
   mid-iteration with prose ("let me run again") instead of final code.
2. **A tight Rust-idiom gap** — the 6 compile errors cluster: ordering trait bounds
   (`Ord` vs `PartialOrd`, sorting `f64`), `mut` params, str-pattern types, return-type
   mismatches. These are learnable patterns, not reasoning depth.

**67% one-shot is the number to beat.** It's a clean read of what the forged 4B writes
cold; the headroom is the ~33% that self-verification + idiom fluency would recover.

## The lever

This is **training-shaped, not knob-shaped.** Per `[[no-hardcoded-heuristics-to-steer-cognition]]`
we do **not** intercept her prose to force an `Act` (that's the puppeting anti-pattern).
The fix is the **genome loop**: this gym is now an objective fitness signal — generate
training traces of correct write→`code/run`→fix iterations (recorder → `dataset/from-turns`
→ `forge/train` → LoRA → genome page-in), targeting (a) emitting `Act` to self-verify by
default and (b) Rust idioms. Re-run the gym to score the delta. That is the `[[coordination-learning-flywheel]]`
made concrete: the gym measures, the genome closes the gap, the gym re-measures.
