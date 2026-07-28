# Benchmark Harness Integration — run the whole being on the benchmarks the market checks

**Status:** design (2026-07-22). Charter: [[beat-oss-agentic-systems-as-whole-beings-never-strip-to-pass]].
Landscape: [[benchmark-campaign-competitive-landscape-2026]].

## The insight from surveying the field

The famous agentic benchmarks are run through **standardized harnesses with a pluggable agent
interface** — we do NOT reinvent the grader per benchmark, and we do NOT strip the being to fit.
Two tracks cover the whole market:

### Track A — Terminal-Bench as the meta-harness (highest leverage)
`harbor-framework/terminal-bench` (tbench.ai) is both a benchmark AND a meta-harness:
- **Dataset registry** already adapts **SWE-bench Verified, DevEval, EvoEval, AppWorld** into the
  TB task framework *without altering task contents* — "same instruction, same environment, same
  unit tests" from the agent's view. One integration → the whole registry.
- **Agent-plugin interface**: implement `AbstractInstalledAgent` (`_install_agent_script_path`,
  `_run_agent_commands`, `_env`, `name`), then `tb run --agent-import-path continuum:ContinuumAgent`.
  Aider, OpenHands, Codex, opencode all plug in the SAME way → apples-to-apples.
- **Model**: any LiteLLM provider → point at our served models (llama-server `/v1`): Qwen3-Coder-30B,
  Devstral, etc. — the models people actually run here.

→ **Build ONE thing: `ContinuumAgent`, a Terminal-Bench agent adapter.** It installs + runs a
headless `continuum-agent` entrypoint that drives a persona's FULL cognition (genome, engrams,
RAG, tools, act→verify loop) against the sandboxed terminal/repo task, as herself. TB grades with
the benchmark's own unit tests. Unlocks Terminal-Bench v2 + SWE-bench Verified + DevEval + EvoEval
+ AppWorld through a single seam.

### Track B — each benchmark's native harness (for the ones not in the TB registry)
- **Aider Polyglot** — 225 Exercism exercises (C++/Go/Java/JS/Python/Rust), Aider's own Docker
  harness: `benchmark/benchmark.py <run> --model X --edit-format whole --exercises-dir
  polyglot-benchmark` (dataset `Aider-AI/polyglot-benchmark`). Run aider-the-arm natively here;
  run OUR being via a thin driver over the same exercise set + the same unit tests.
- **SWE-bench Verified/Pro** — also runnable via the standardized `mini-SWE-agent` harness (the
  fair-comparison harness; OpenHands' "own harness" 77.6% vs mini-SWE-agent ~72–76% shows why the
  harness must be held constant). Prefer the TB-registry SWE-bench Verified for one seam; keep the
  native SWE-bench harness for Pro.
- **LiveCodeBench / HumanEval / MBPP** — contamination-free / classic code-gen; run through our own
  ergonomic `benchmark/*` gym (rustc/pytest grader) with the stub layered into her RAG. Table stakes.

## The persona plugs in as an AGENT, never a bare LLM

For every harness the task input (a terminal instruction, a SWE-bench repo+issue, an Exercism stub)
is **layered into her situation/RAG** ([[situation-aware-focuser]]); she operates IN the sandbox
with her tools and loop; the harness's own grader scores the result. Being constant, RAG adapts to
the harness shape ([[benchmark-must-never-score-persona-against-a-soul-stripped-copy]]). The bare
model is only ever a labeled FLOOR line, never a peer.

## Competitor arms (how people actually run them — surveyed)
| Agent | Native run | Standard harness |
|---|---|---|
| Hermes | `hermes -z "<task>" --provider lmstudio LM_BASE_URL=<our /v1>` | TB agent adapter |
| Aider | `aider --model X --message …` in a git repo | Aider Polyglot Docker harness; TB adapter |
| OpenHands | `run_controller()` + `user_response_fn` | ships SWE-bench adapter; TB adapter |
| OpenCode | headless terminal run | TB agent adapter |
| Codex CLI | `codex -q` | TB agent adapter (leads TB 2.1) |
All are `/v1`-compatible → same CompetitorAgent adapter pattern (subprocess or `/v1`) as Hermes.

## Build sequence
1. **`agent/solve` headless entrypoint** — ✅ DONE + live-proven (2026-07-22, commits `4664ad7ac`
   + rooting `0e9686e86` + detach). `cu agent/solve --persona-id … --base-model-id … --task … --workspace …`
   drives a persona's FULL cognition (tools on, recall on, genome on the measurement lane) to
   completion in the sandbox cwd and returns `{acts, spoken, patch (git diff), files_changed}`.
   Never strips faculties. Her hands are rooted at the caller's cwd via the shared
   `root_acting_workspace` seam (fail-loud). Long agentic drives use `--detach true` → instant ack
   with a `run_id`, real result polled from `~/.continuum/progress/agent-solve-<run_id>.json`
   (#86 fire-and-poll; a Devstral drive took 12 min — MUST NOT block the socket).
2. **`ContinuumAgent` Terminal-Bench adapter** (`benchmarks/terminal-bench/continuum_agent.py`) →
   `tb run` gives us Terminal-Bench + SWE-bench Verified + DevEval + EvoEval + AppWorld. **← NEXT.**
   Shells `cu agent/solve --detach` in the sandbox, polls the ledger, applies the patch.
3. **Model matrix** = LiteLLM → served {Qwen3-Coder-30B, Devstral, …}. **Agent matrix** = {Continuum,
   Hermes, Aider, OpenHands, OpenCode} + bare-model floor. Same harness, same tasks, same grader.
4. **Aider Polyglot** track (Docker harness) + **SWE-bench Pro** (mini-SWE-agent harness) for the
   two credibility numbers not in the TB registry.
5. **Local inner-loop** stays our fast ergonomic `benchmark/*` rustc gyms for iteration; the famous
   harnesses are the marketing spine. Both feed the headline table.

## Marketing target (the table that must exist)
Continuum (whole being) vs {Hermes, Aider, OpenHands, OpenCode} vs bare-model floor, across
{Terminal-Bench v2, SWE-bench Verified, SWE-bench Pro, Aider Polyglot, LiveCodeBench, HumanEval},
on {Qwen3-Coder-30B, Devstral, …}. Recognized rows, standardized harnesses, reproducible commands.
