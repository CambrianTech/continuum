# All benchmark results — the complete ledger

Rendered from [`RESULTS.jsonl`](RESULTS.jsonl) by `render_results.py`. Append-only: every run ever recorded is here, newest first. The README shows the latest row per cell; this page shows them all.

## Lab-grade (external)

| captured | benchmark | model | arm | result | machine | git sha | note |
|---|---|---|---|---|---|---|---|
| 2026-08-25 | Terminal-Bench 2.1 | Ornith-1.5-35B-A3B | OURS | 100% (7/7) | macbook-m5-64gb | `e06241469` | ROUND 2 — THE RETAKE CURVE (memory claim, disclosed: same 7 gold-gated tasks as round 1's 71%; fresh-task transfer curve is the separate obligation as env fixes unlock the 46 blocked tasks). One night of lessons between rounds, no code changes to her mind mid-round. Act counts round1→round2: cancel-async env-fail→PASS(21), code-from-image 16→2, db-wal 18→23, batching-scheduler MISS→PASS(20) — the task BOTH arms (incl. mini-swe-stock) missed fresh, cracked via her own lesson carrying the oracle's FileNotFoundError evidence. Beats mini-swe-stock's 86% first-sight (their harness cannot retake — frozen scaffolds have a score, ours has a slope). Run d4bfc787; replication: continuum benchmark/round --benchmark tb21-gold7.jsonl --persona <id> --fresh true |
| 2026-08-25 | Terminal-Bench 2.1 | Ornith-1.5-35B-A3B | OURS | 100% (9/9) | macbook-m5-64gb | `e06241469` | CUMULATIVE gold-gated coverage after round 2 + TRANSFER round: 9/9 distinct tasks passed at latest attempt (7 retake + 2 NEVER-SEEN: constraints-scheduling 7 acts first-sight, extract-moves-from-video 1 act first-sight — the fresh-task/transfer curve's opening points; single-act video pass flagged for post-mortem verification of oracle strictness per standing rhythm). Retake curve and transfer curve now BOTH nonzero — memory AND generalization measured separately. Runs 706215f4+d4bfc787+08b95e84. |
| 2026-08-24 | Terminal-Bench 2.1 | Ornith-1.5-35B-A3B | OURS | *pending* | macbook-m5-64gb | `c981a0879` | gold-gated 7/53 subset (46 env-blocked tasks named in tb-goldgate.csv); round d9d9c555 IN FLIGHT — one-command round, learn mode, lived sampling |
| 2026-08-24 | Terminal-Bench 2.1 | Ornith-1.5-35B-A3B | mini-swe-stock | *pending* | macbook-m5-64gb | `c981a0879` | QUEUED after OURS: mini-swe-agent 2.4.6 + unmodified ggml-org llama-server b10612, default flags (-c 65536 documented exception) — their whole world |
| 2026-08-24 | Terminal-Bench 2.1 | Ornith-1.5-35B-A3B | OURS | 71% (5/7) | macbook-m5-64gb | `e06241469` | GOLD-GATED subset (7 of 53 tasks provably gradeable on this host; 46 env-blocked tasks named in receipts — NOT a 53-task claim). Run 706215f4: 5 passes incl. five consecutive after the mid-round working-memory fix (act counts 32→16→18→11→5→11); 2 misses: cancel-async-tasks = harness env-fail (bare python → exit 127; interpreter projection fixed, retake owed), llm-inference-batching-scheduler = budget exhaustion on the pre-fix starved mind (retake owed). Round interrupted+resumed 3x with all grades kept (one-command round). Replication: continuum benchmark/round --benchmark tb21-gold7.jsonl --persona <id> |
| 2026-08-24 | Terminal-Bench 2.1 | Ornith-1.5-35B-A3B | mini-swe-stock | 86% (6/7) | macbook-m5-64gb | `e06241469` | THEIR WHOLE WORLD: mini-swe-agent 2.4.6 + unmodified ggml-org llama-server b10612 default flags (-c 65536 documented exception), same 7 gold-gated tasks + oracles, ~5 min/task. HONEST READING: on the 6 tasks where both arms had fair envs it is DEAD EVEN (5 passes each, both missed llm-inference-batching-scheduler); the 1-task gap is cancel-async-tasks, which this arm sat on the interpreter-FIXED harness while OURS was graded on the broken one — OURS retake pending. Driver receipts: 3 harness-env bugs on the opponent's side were fixed before any row minted (wizard, cost-tracker, exit prompt) — same env-fail protection both directions. |
| 2026-07-23 | SWE-bench Lite | unsloth/Devstral-Small-2507-GGUF | OURS-agent | 0% (0/1) | Joels-MacBook-Pro | `69c63890f` | flask-4045 UNRESOLVED (official harness, image-local run). Near-miss anatomy: found the EXACT gold fix (ValueError on dotted Blueprint name, correct site) but smeared it — 2 duplicate insertions incl. a broken trailing __init__ redefinition that breaks the class. Defect class: append-style edits instead of precise replacement + never ran the repo's test suite before finishing. Both are verify-correct gene targets (training queued tonight). |
| 2026-07-23 | SWE-bench Lite | unsloth/Devstral-Small-2507-GGUF | OURS-team | 0% (0/1) | Joels-MacBook-Pro | `51ffa2620` | flask-4045 team arm UNRESOLVED but a DIFFERENT, better miss than solo: Anwen's re-solve was CLEAN (19 acts vs 40; single correct-site insertion, 488B vs 1959B smear — first run after learn-mode admitted her run-1 experience). Asha's review (40 acts) preserved it. Remaining gap: fix covers dotted NAME only; gold also validates dotted ENDPOINTS (add_url_rule) — the FAIL_TO_PASS endpoint test still fails. Also 111 spurious werkzeug-deprecation errors polluted her test-feedback signal. |
| None | SWE-bench Lite | Devstral-Small-24B | OURS | *pending* | macbook-m-series | `pending` | harness wired (benchmarks/swe/run_ours.py), official swebench scorer — run pending |

## Whole-being battery

| captured | benchmark | model | arm | result | machine | git sha | note |
|---|---|---|---|---|---|---|---|
| 2026-07-22 | Agent-Solve Tier 1 | Qwen2.5-Coder-7B | OURS | 60% (6/10) | Joels-MacBook-Pro.local | `9e5e556c4` | whole-being agent/solve battery, 2/2 clean reps, memory ON, never stripped |
| 2026-07-22 | Agent-Solve Tier 2 | Qwen2.5-Coder-7B | OURS | 20% (2/10) | Joels-MacBook-Pro.local | `9e5e556c4` | whole-being agent/solve battery, 2/2 clean reps, memory ON, never stripped |
| 2026-07-22 | Agent-Solve Tier 1 | Qwen2.5-Coder-7B | OURS | 53% (8/15) | Joels-MacBook-Pro.local | `d89171cc6` | whole-being agent/solve battery, 3/3 clean reps, memory ON, never stripped |
| 2026-07-22 | Agent-Solve Tier 2 | Qwen2.5-Coder-7B | OURS | 0% (0/15) | Joels-MacBook-Pro.local | `d89171cc6` | whole-being agent/solve battery, 3/3 clean reps, memory ON, never stripped |
| 2026-07-22 | Agent-Solve Tier 1 | Qwen2.5-Coder-7B | OURS | 93% (14/15) | Joels-MacBook-Pro.local | `1406e2ad7` | whole-being agent/solve battery, 3/3 clean reps, memory ON, never stripped |
| 2026-07-22 | Agent-Solve Tier 2 | Qwen2.5-Coder-7B | OURS | 7% (1/15) | Joels-MacBook-Pro.local | `1406e2ad7` | whole-being agent/solve battery, 3/3 clean reps, memory ON, never stripped |
| 2026-07-22 | Agent-Solve Tier 1 | Qwen2.5-Coder-7B | OURS | 87% (13/15) | Joels-MacBook-Pro.local | `0c20177ef` | whole-being agent/solve battery, 3/3 clean reps, memory ON, never stripped |
| 2026-07-22 | Agent-Solve Tier 2 | Qwen2.5-Coder-7B | OURS | 0% (0/15) | Joels-MacBook-Pro.local | `0c20177ef` | whole-being agent/solve battery, 3/3 clean reps, memory ON, never stripped |

## Fast verifiable gyms

| captured | benchmark | model | arm | result | machine | git sha | note |
|---|---|---|---|---|---|---|---|
| 2026-08-23 | Hard-Rust | ornith-ai/Ornith-1.5-35B-A3B-GGUF | OURS-citizen | 75% (6/8) | Joels-MacBook-Pro-M5-64GB | `d4d26a717` | Take 3 — first honest hard-rs number (takes 1-2 infra-poisoned by #2382/#2384 grading defects, discarded unrecorded). Citizen live cognition via cognition/eval, #312 ephemeral CoW root, glass-boxed both directions (fail hand-verified genuine, pass = defect-2's witness task). Contention caveat: Atlas's own SWE round ran concurrently (reboot wiped sleep mode) — 45.7% mean cache hit, 47 tok/s mean decode. run 0fd92d32. |
| 2026-07-23 | Hard-Rust | Hermes-3-Llama-3.1-8B | OURS | 12% (1/8) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | Hard-Rust | Qwen2.5-Coder-1.5B | OURS | 0% (0/8) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | Hard-Rust | Qwen2.5-Coder-3B | OURS | 25% (2/8) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | Hard-Rust | qwen3.5-4b-code-forged (OURS-forged) | OURS | 25% (2/8) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | Hard-Rust | Qwen2.5-Coder-14B | RAW | *excluded* | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | Hard-Rust | Qwen2.5-Coder-14B | OURS | 50% (4/8) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | Hard-Rust | Devstral-Small-24B | RAW | *excluded* | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | Hard-Rust | Devstral-Small-24B | OURS | 38% (3/8) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-22 | Hard-Rust | Qwen2.5-Coder-14B | opencode | 0% (0/8) | Joels-MacBook-Pro.local | `27b28e71f` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | RAW | 33% (1/3) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | OURS | 0% (0/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | opencode | 0% (0/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | hermes | 0% (0/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | aider | 0% (0/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | RAW | 33% (1/3) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | OURS | 0% (0/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | opencode | 0% (0/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | hermes | 0% (0/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | aider | 0% (0/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | RAW | 12% (1/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | OURS | 0% (0/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | opencode | 0% (0/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | hermes | 12% (1/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Hermes-3-Llama-3.1-8B | aider | 0% (0/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Devstral-Small-24B | RAW | 38% (3/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Devstral-Small-24B | OURS | 38% (3/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Devstral-Small-24B | opencode | 50% (4/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Devstral-Small-24B | hermes | 50% (4/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-14 | Hard-Rust | Devstral-Small-24B | aider | 38% (3/8) | Joels-MBP.lan | `1859bc615` | None |
| 2026-07-11 | Hard-Rust | Hermes-3-Llama-3.1-8B | OURS | 0% (0/8) | Joels-MBP.lan | `e9d2ec1c2` | None |
| 2026-07-11 | Hard-Rust | Qwen2.5-Coder-1.5B | OURS | 0% (0/8) | Joels-MBP.lan | `e9d2ec1c2` | None |
| 2026-07-11 | Hard-Rust | Qwen2.5-Coder-3B | OURS | 25% (2/8) | Joels-MBP.lan | `e9d2ec1c2` | None |
| 2026-07-11 | Hard-Rust | qwen3.5-4b-code-forged (OURS-forged) | OURS | *excluded* | Joels-MBP.lan | `e9d2ec1c2` | None |
| 2026-07-11 | Hard-Rust | Qwen2.5-Coder-14B | RAW | *excluded* | Joels-MBP.lan | `e9d2ec1c2` | None |
| 2026-07-11 | Hard-Rust | Qwen2.5-Coder-14B | OURS | 62% (5/8) | Joels-MBP.lan | `e9d2ec1c2` | None |
| 2026-07-11 | Hard-Rust | Qwen2.5-Coder-14B | opencode | 0% (0/8) | Joels-MBP.lan | `e9d2ec1c2` | None |
| 2026-07-10 | Hard-Rust | Devstral-Small-24B | RAW | 38% (3/8) | macbook-m-series | `seed` | None |
| 2026-07-10 | Hard-Rust | Devstral-Small-24B | OURS | 38% (3/8) | macbook-m-series | `seed` | zero tax on the hard board |
| 2026-07-10 | Hard-Rust | Qwen2.5-Coder-3B | OURS | 25% (2/8) | macbook-m-series | `seed` | None |
| 2026-07-10 | Hard-Rust | Qwen2.5-Coder-14B | OURS | *excluded* | macbook-m-series | `seed` | EXCLUDED: degenerate lane (2 tok/task) under GPU contention — serving, not model |
| None | Frontier-Rust | Devstral-Small-24B | OURS | *pending* | macbook-m-series | `pending` | gym committed + reference-verified — run pending |

## Retired gyms (history preserved; no longer on the README face)

| captured | benchmark | model | arm | result | machine | git sha | note |
|---|---|---|---|---|---|---|---|
| 2026-07-23 | HumanEval-Rust | Hermes-3-Llama-3.1-8B | OURS | 35% (14/40) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | HumanEval-Rust | Qwen2.5-Coder-1.5B | OURS | 48% (19/40) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | HumanEval-Rust | Qwen2.5-Coder-3B | OURS | 68% (27/40) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | HumanEval-Rust | qwen3.5-4b-code-forged (OURS-forged) | OURS | 62% (25/40) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | HumanEval-Rust | Qwen2.5-Coder-14B | RAW | *excluded* | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | HumanEval-Rust | Qwen2.5-Coder-14B | OURS | 92% (37/40) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | HumanEval-Rust | Devstral-Small-24B | RAW | 0% (0/1) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-23 | HumanEval-Rust | Devstral-Small-24B | OURS | 88% (35/40) | Joels-MacBook-Pro.local | `5d9adfde0` | None |
| 2026-07-10 | HumanEval-Rust | Qwen2.5-Coder-14B | RAW | 86% (43/50) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | Qwen2.5-Coder-14B | OURS | 92% (46/50) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | Qwen2.5-Coder-14B | opencode | *excluded* | Joels-MBP.lan | `01b48fbdc` | shim-bypassed (raw endpoint, no toolcall_shim) — unfair, re-run pending |
| 2026-07-10 | HumanEval-Rust | Hermes-3-Llama-3.1-8B | RAW | 52% (21/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | Hermes-3-Llama-3.1-8B | OURS | 38% (15/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | Hermes-3-Llama-3.1-8B | opencode | *excluded* | Joels-MBP.lan | `01b48fbdc` | shim-bypassed (raw endpoint, no toolcall_shim) — unfair, re-run pending |
| 2026-07-10 | HumanEval-Rust | Hermes-3-Llama-3.1-8B | hermes | 22% (9/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | Hermes-3-Llama-3.1-8B | aider | 48% (19/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | Qwen2.5-Coder-1.5B | RAW | 45% (18/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | Qwen2.5-Coder-1.5B | OURS | 50% (20/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | Qwen2.5-Coder-1.5B | opencode | *excluded* | Joels-MBP.lan | `01b48fbdc` | shim-bypassed (raw endpoint, no toolcall_shim) — unfair, re-run pending |
| 2026-07-10 | HumanEval-Rust | Qwen2.5-Coder-1.5B | aider | 50% (20/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | Qwen2.5-Coder-3B | RAW | 32% (13/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | Qwen2.5-Coder-3B | OURS | 72% (29/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | Qwen2.5-Coder-3B | opencode | *excluded* | Joels-MBP.lan | `01b48fbdc` | shim-bypassed (raw endpoint, no toolcall_shim) — unfair, re-run pending |
| 2026-07-10 | HumanEval-Rust | Qwen2.5-Coder-3B | aider | 80% (32/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | qwen3.5-4b-code-forged (OURS-forged) | RAW | 70% (28/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | qwen3.5-4b-code-forged (OURS-forged) | OURS | 30% (12/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | qwen3.5-4b-code-forged (OURS-forged) | opencode | *excluded* | Joels-MBP.lan | `01b48fbdc` | shim-bypassed (raw endpoint, no toolcall_shim) — unfair, re-run pending |
| 2026-07-10 | HumanEval-Rust | qwen3.5-4b-code-forged (OURS-forged) | hermes | 62% (25/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-10 | HumanEval-Rust | qwen3.5-4b-code-forged (OURS-forged) | aider | 72% (29/40) | Joels-MBP.lan | `01b48fbdc` | None |
| 2026-07-08 | HumanEval-Rust | Qwen2.5-Coder-14B | RAW | 90% (18/20) | macbook-m-series | `seed` | proof-of-method slice |
| 2026-07-08 | HumanEval-Rust | Qwen2.5-Coder-14B | OURS | 90% (18/20) | macbook-m-series | `seed` | zero tax vs RAW |
| 2026-07-08 | HumanEval-Rust | Qwen2.5-Coder-14B | opencode | 75% (15/20) | macbook-m-series | `seed` | opencode loop drops 3 tasks the model gets raw |
| 2026-07-08 | HumanEval-Rust | Devstral-Small-24B | RAW | 100% (5/5) | macbook-m-series | `seed` | None |
| 2026-07-08 | HumanEval-Rust | Devstral-Small-24B | OURS | 100% (5/5) | macbook-m-series | `seed` | zero tax after tool-surface fix |
| 2026-07-08 | HumanEval-Rust | Hermes-3-Llama-3.1-8B | OURS | 52% (21/40) | macbook-m-series | `seed` | fixed opponent baseline |
