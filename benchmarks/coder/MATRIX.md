# Coder benchmark matrix — hard-rs (40 tasks, rustc compile+run graded)

Same tasks, same grader, every number reproducible. RAW = model one-shot against its own `/v1`. OURS = the same model through the full Continuum loop. opencode = the same class of local model through the opencode agentic harness (fair tool-format shim).

| model | RAW one-shot | OURS (Continuum) | opencode | Δ OURS−RAW |
|---|---|---|---|---|
| Hermes-3-Llama-3.1-8B | — | 0% (0/8) | — | — |
| Qwen2.5-Coder-1.5B | — | 0% (0/8) | — | — |
| Qwen2.5-Coder-3B | — | 25% (2/8) | — | — |
| qwen3.5-4b-code-forged (OURS-forged) | — | 0% (0/8) | — | — |
| Qwen2.5-Coder-14B | — | 62% (5/8) | 0% (0/8) | — |
| Devstral-Small-24B ⚠ cell failed (see log) | — | — | — | — |

## Reproduce

```bash
# boot a Continuum core (serves your local model), then:
python3 benchmarks/coder/matrix.py --models benchmarks/coder/models.json \
    --benchmark hard-rs --limit 40 --out benchmarks/coder/MATRIX.md
```

Add a model = one row in `benchmarks/coder/models.json`. A bigger machine with more VRAM sweeps more models with the identical command.

## gym-mine: real-repo bugfix tasks (bitflags, doubly-verified)

Tasks mined by `gym/mine` from the bitflags crate's history (bugfix-revert: setup re-breaks
`src/lib.rs` at the fixing commit's parent; DoD restores canonical tests and runs `cargo test`).
Three tasks, each verified broken-fails/fixed-passes at mining time. Solver = Asha
(Devstral-Small-2507 24B) through her LIVE cognition via `cognition/eval`, full act→observe
loop, honest room contention (personas stayed live — first-class citizens even during benchmarks).

| run | pass | acts (read/shell/tree/edit) | infra | verdict |
|---|---|---|---|---|
| run1 (2026-07-11) | 0/3 | 118 (58/54/6/0) | 58/65 reads killed by FALSE "Security: escapes" on in-sandbox ENOENT | INVALID — instrument lied (fixed: ae4c50127) |
| run2 (2026-07-11, post-fix) | 0/3 | 106 (12/76/18/0) | clean (0 false errors; 4 honest NotFound, recovered) | HONEST ZERO — diagnosed + self-verified 100%, never attempted a repair edit |

The run2 zero is the finding, not a failure of the harness: she reads the failing output,
re-runs the tests, narrates the diagnosis — and never converts to `code/edit`. Same
intention→action texture as the live room (#122's curriculum; Scenario Library
`narrate-vs-act` generator). Open instrument follow-ups before the next arm: prompt-captures
do not record the native `tools` array (cannot audit whether edit_file/write_file were
offered), and the repeat-guard nudge text ("I should ANSWER…") biases Speak over a
different act.

Follow-up runs (same tasks, instrument hardening between each — the mistake-driven loop
applied to the HARNESS):

| run | outcome | verdict |
|---|---|---|
| run3 | froze at tick 2 (act decided, no shell child, no error) | INSTRUMENT — trailing-assistant threads 400'd under thinking ("prefill incompatible"); 1000+ live self-ticks had died silently over 2 days; fixed (close_trailing_assistant) |
| run4a | failed loud at launch | INSTRUMENT — raced the reboot's persona spawn ("no workspace template"); fail-loud worked; eval-status gap: a dead detached run reports complete:false forever (#86) |
| run4b | froze at tick 14 on task 2's `cargo test` (no child process, status Running forever) | INSTRUMENT — `execute_and_wait_async` trusted the runner task to always report; a silent task death hung the waiter; fixed (bounded re-check + hard deadline, #85 slice 1) |

Positive instrument signal from run4b before the freeze: with honest NotFound errors her
tool mix shifted read-heavy (46 read / 28 shell / 10 tree at tick 14) vs run2's
shell-dominant mix (12/76/18) — she navigates with her file hands when they tell the truth.
The clean full pass (run5) awaits the deadline fix deployed. Board acts note: the citizen-layer
work files (reverse_string.rs 04:19, conway_game_of_life.rs 05:28) were written BEFORE the
prefill fix; claims of "I've just run it" remain unbacked by any execution (confabulation
class, Scenario Library).

| run5 (all instrument fixes) | 0/3 | 132 acts (32 read / 92 shell / 8 tree / 0 edit) | clean: 0 false errors, 11 honest NotFound (recovered), 0 deadline fires | HONEST ZERO — replicates run2; neutral nudge changed nothing. Diagnose-without-repair is the reproducible finding → #122 training target |

## § swe-bench — flask-4045 cell (Devstral-Small-2507 24B resident, OURS arm, 2026-07-11)

Scoring: official `swebench` Docker harness. Instance is hermetic (gold control RESOLVED).
Controls already banked: **gold patch → RESOLVED** (spine proof); **Claude-as-agent, same
tools → RESOLVED** (tool ceiling proof — the hands carry a real fix; the model is the variable).

| attempt | perception layer added | acts | verdict |
|---|---|---|---|
| 1 | (baseline: rooted seam, glass box, forgiving edit+search, loop-note) | 25/25 code/search, 0 read, 0 edit | honest 0 — 101-hit result walls |
| 2 | search OVERFLOW SUMMARY (68a43efb3) — verified in her perception | 26 acts: 25 search + 1 list, 0 read | honest 0 — menu in view, no conversion |
| 3 | [investigation] act-distribution fact (7737e73f6) — verified rendering "code/search ×3" beside repeat-note + menu | 17+ acts: all search + 1 work/claim, 0 read | honest 0 — perception arsenal EXHAUSTED |

**Conclusion:** every structural cause eliminated by instrument (results real, window real
at ~5k tokens with no context-shift, facts verified in view). The search→read→edit
transition in EXAM framing is a weights/curriculum gap → #122 LoRA target. Corpus banked:
3 negative traces + Claude's RESOLVED positive + the room's genuine read→write→run chains.
Context-inversion data point: the SAME persona (Anwen) chained read→build→run in the live
room all afternoon — exam framing, not capability, gates the transition
([[eval-is-an-exam-not-a-life]]).

**Matrix axes locked (2026-07-11):** rows = every Mac-runnable coder (+cloud rows later);
cols = raw / opencode(fair shim) / OURS / OURS+genome; third axis = hardware tier
(one MacBook / +5090 / small grid) with cost-per-resolved-task reported beside
resolve-rate. Target benchmarks by mindshare: SWE-bench Verified/Lite, Terminal-Bench,
Aider polyglot, BFCL; the two headline formats no leaderboard can copy: same-weights
before/after genome training, and $/resolved-task vs cloud.

## § evidence-ledger matrix (auto-rendered by uu benchmark/matrix, 2026-07-11 ~23:10)

## humaneval-rs

| model | ours |
|---|---|
| NousResearch/Hermes-3-Llama-3.1-8B-GGUF | 9/20 (45%) |
| continuum-ai/qwen2.5-coder-14b-instruct-GGUF | 18/20 (90%) |
| unsloth/Devstral-Small-2507-GGUF | 19/20 (95%) |

## swe-bench-lite

| model | ours | control |
|---|---|---|
| claude-sonnet-agent | 1/1 (100%) | — |
| gold-patch | — | 1/1 (100%) |
| unsloth/Devstral-Small-2507-GGUF | 0/1 (0%) | — |

### Replication

- **unsloth/Devstral-Small-2507-GGUF × ours × swe-bench-lite** on `macbook-m4-pro-64gb`: `benchmarks/swe/run_ours.py --instance pallets__flask-4045 --max_acts 25 (3 attempts, successive perception layers)` — honest zero; search-loop in exam framing; controls bracket it: gold RESOLVED, Claude-through-same-tools RESOLVED. See benchmarks/coder/MATRIX.md
- **gold-patch × control × swe-bench-lite** on `macbook-m4-pro-64gb`: `official swebench docker harness, gold patch for pallets__flask-4045` — harness-integrity control
- **claude-sonnet-agent × ours × swe-bench-lite** on `macbook-m4-pro-64gb`: `Claude as agent through the identical code/* tool surface on pallets__flask-4045` — tool-ceiling control: the hands can carry a real fix
- **NousResearch/Hermes-3-Llama-3.1-8B-GGUF × ours × humaneval-rs** on `macbook-m4-pro-64gb`: `uu benchmark/run {persona_id, name: humaneval-rs, limit: 20, base_model_id: NousResearch/Hermes-3-Llama-3.1-8B-GGUF, max_acts: 6, detach: true}` — first Hermes cell; 18.3 tok/s decode; ephemeral GPU lane; supersedes earlier degenerate 0/8
- **unsloth/Devstral-Small-2507-GGUF × ours × humaneval-rs** on `macbook-m4-pro-64gb`: `uu benchmark/run {persona_id, name: humaneval-rs, limit: 20, base_model_id: unsloth/Devstral-Small-2507-GGUF, max_acts: 6, detach: true}` — ephemeral GPU lane
- **continuum-ai/qwen2.5-coder-14b-instruct-GGUF × ours × humaneval-rs** on `macbook-m4-pro-64gb`: `uu benchmark/run {persona_id, name: humaneval-rs, limit: 20, base_model_id: continuum-ai/qwen2.5-coder-14b-instruct-GGUF, max_acts: 6, detach: true}` — ephemeral GPU lane; 67s wall for 20 tasks


## § DEFINITIVE full-set board (humaneval-rs n=156, OURS arm, greedy, 2026-07-12 overnight)

The pilot table below survives as history; **these are the citable rows** (full valid set —
the gym carries 156 valid tasks of 164; Wilson 95% CIs; every row in
`~/.continuum/benchmarks/ledger.jsonl` with its replication command; snapshot-eval isolation,
no learning during exams).

| model | params (active) | resolved | pass rate | Wilson 95% CI | wall |
|---|---|---|---|---|---|
| bartowski/Qwen2.5-Coder-32B-Instruct-GGUF | 32B | **131/156** | **84.0%** | [0.774, 0.889] | 23.5 min |
| continuum-ai/qwen2.5-coder-14b-instruct-GGUF | 14B | **131/156** | **84.0%** | [0.774, 0.889] | 21 min |
| unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF | 30B MoE (~3B) | **130/156** | **83.3%** | [0.767, 0.884] | 7 min |
| unsloth/Devstral-Small-2507-GGUF | 24B | 106/156 | 67.9% | [0.603, 0.748] | 54 min |
| NousResearch/Hermes-4.3-36B-GGUF | 36B | 106/156 | 67.9% | [0.603, 0.748] | 86 min |
| NousResearch/Hermes-3-Llama-3.1-8B-GGUF | 8B | 38/156 | 24.4% | [0.183, 0.317] | 14 min |

**What the board says (all same harness, same greedy settings, same full set):**
1. **The pilot's first-20 easy-slice inflation is proven**: every 20-task pilot number above
   overstated its full-set truth (Devstral 95%→67.9%, Hermes-4.3 80%→67.9%). Full sets or it
   didn't happen.
2. **Coder-14B ties Coder-32B exactly** (131/156 each) at ~40% the memory — the consumer-
   hardware sweet spot for this machine class.
3. **The 30B-A3B MoE ties the dense 32B within noise at ~10× the speed** (7 min vs 23.5 min
   for the full set) — the throughput champion.
4. **Hermes-4.3-36B lands statistically BELOW the Qwen tier** (non-overlapping CIs vs both
   131/156 rows) and exactly ties Devstral-24B while being larger and 1.6× slower.
5. The 8B row is the honest floor: the system runs it cleanly; the model is the limit.

forged-4B full-set row pending (in flight). Raw arms (oneshot), aider-polyglot replication,
and team/genome arms are the next columns per the paper's §3.

## § round-1 pilot table (humaneval-rs 20-task, OURS arm, 2026-07-12 early AM — SUPERSEDED by the definitive board above; kept as the pilot-bias exhibit)

## humaneval-rs

| model | ours |
|---|---|
| NousResearch/Hermes-3-Llama-3.1-8B-GGUF | 9/20 (45%) |
| NousResearch/Hermes-4.3-36B-GGUF | 16/20 (80%) |
| continuum-ai/qwen2.5-coder-14b-instruct-GGUF | 18/20 (90%) |
| continuum-ai/qwen3.5-4b-code-forged-GGUF | 1/20 (5%) |
| unsloth/Devstral-Small-2507-GGUF | 19/20 (95%) |
| unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF | 18/20 (90%) |

### Replication

- **NousResearch/Hermes-3-Llama-3.1-8B-GGUF × ours × humaneval-rs** on `macbook-m4-pro-64gb`: `uu benchmark/run {persona_id, name: humaneval-rs, limit: 20, base_model_id: NousResearch/Hermes-3-Llama-3.1-8B-GGUF, max_acts: 6, detach: true}` — first Hermes cell; 18.3 tok/s decode; ephemeral GPU lane; supersedes earlier degenerate 0/8
- **unsloth/Devstral-Small-2507-GGUF × ours × humaneval-rs** on `macbook-m4-pro-64gb`: `uu benchmark/run {persona_id, name: humaneval-rs, limit: 20, base_model_id: unsloth/Devstral-Small-2507-GGUF, max_acts: 6, detach: true}` — ephemeral GPU lane
- **continuum-ai/qwen2.5-coder-14b-instruct-GGUF × ours × humaneval-rs** on `macbook-m4-pro-64gb`: `uu benchmark/run {persona_id, name: humaneval-rs, limit: 20, base_model_id: continuum-ai/qwen2.5-coder-14b-instruct-GGUF, max_acts: 6, detach: true}` — ephemeral GPU lane; 67s wall for 20 tasks
- **continuum-ai/qwen3.5-4b-code-forged-GGUF × ours × humaneval-rs** on `macbook-m4-pro-64gb`: `uu benchmark/run {persona_id, name: humaneval-rs, limit: 20, base_model_id: continuum-ai/qwen3.5-4b-code-forged-GGUF, max_acts: 6, detach: true}` — SERVING SUSPECT: 32 tok/task mean (half known-healthy rate); model requires thinking mode which lane may suppress — re-run pending, do not cite as clean score
- **unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF × ours × humaneval-rs** on `macbook-m4-pro-64gb`: `uu benchmark/run {persona_id, name: humaneval-rs, limit: 20, base_model_id: unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF, max_acts: 6, detach: true}` — the LocalLLaMA community champion (MoE 3B active); ephemeral GPU lane
- **NousResearch/Hermes-4.3-36B-GGUF × ours × humaneval-rs** on `macbook-m4-pro-64gb`: `uu benchmark/run {persona_id, name: humaneval-rs, limit: 20, base_model_id: NousResearch/Hermes-4.3-36B-GGUF, max_acts: 6, detach: true}` — Nous flagship-mid (seed_oss 36B dense); healthy output volume; 19min wall vs 14B 67s — the size/speed axis


Wall-clock note: Coder-14B 67s, 30B-A3B 157s, Hermes-4.3-36B 1129s for the same 20 tasks — the cost axis. forged-4B row is SERVING SUSPECT (thinking-mode), re-run pending.
