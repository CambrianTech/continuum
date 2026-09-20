# Rival CLI local-model configs — what their communities actually run

Researched 2026-07-23 (the "known-good config verification" prerequisite of the
rival-arm integrity standard, benchmarks/agent-solve/README.md). Every claim
cited; the harnesses must match these shapes before any rival cell publishes.

## opencode

- **Community path:** custom provider via `@ai-sdk/openai-compatible` pointed at
  any OpenAI-compatible base (Ollama `:11434/v1`, LM Studio `:1234/v1`, or a
  llama-server). Our shim approach is equivalent in shape.
- **THE FINDING — context:** opencode documents an expectation of **64K+
  context**, and community guidance is explicit that **tool calls break below
  ~16–32K** (fix = raise `num_ctx`). Our matrix served 32K-native coders at
  their true trained context — squarely in the silent-breakage zone. This is
  the leading root-cause hypothesis for every opencode `excluded`/0% cell:
  **not model capability, and not exactly our bug either — a documented rival
  requirement we unknowingly violated.**
- **Models their users pick:** larger Qwen coder variants and Gemma-class
  models with raised contexts; small models are called out as weak for it.
- **Implication for the matrix:** opencode cells are only FAIR on models
  serving ≥64K (Devstral-Small at its long context; our compacted 19B @256K;
  any coder with a genuine long-context build). A 32K-native model gets a
  *refused-by-rival-requirement* cell — the same absent-not-zero treatment the
  Hermes footnote already established. (And the same editorial point applies:
  Continuum runs the 32K-native coders these tools can't.)

## Hermes (Nous CLI)

- **Hard-refuses models under 64K context** — discovered empirically by us,
  consistent with the ecosystem norm above; already footnoted in the README
  table as absent-not-zero. Scores when its requirement is met (e.g. 50%
  hard-rs on Devstral). Integration considered VERIFIED within its rules.

## aider

- **Community path:** LiteLLM/OpenAI-compatible against llama-server —
  `OPENAI_API_BASE` + `openai/<model>`, exactly the shape our harness uses.
  Community notes favor ~64K context for comfort but aider degrades gracefully
  rather than breaking tool flow (it edits files via diffs, not tool-calls).
- **Status: VERIFIED** — scores across five models in our ledger, beats us on
  rows we published. The reference proof that our rival-driving CAN be fair.

## The revised fair-matrix protocol

1. Rival arms run on the **≥64K-serving subset** of the model grid; sub-64K
   models get *refused-by-requirement* cells (absent, footnoted, never 0).
2. Smoke gate before every battery (rival_integrity.py) — an integration that
   can't produce `fn main() {}` VOIDs the run with the reason printed.
3. Per-task rival-INFRA excluded from denominators; majority-infra = VOID.
4. Re-run the boards under this protocol before publishing any Δ column.

Sources: [opencode providers](https://opencode.ai/docs/providers/) ·
[Ollama × OpenCode](https://docs.ollama.com/integrations/opencode) ·
[opencode local setup guides](https://haimaker.ai/blog/ollama-opencode-setup/) ·
[local-aider reference setup](https://github.com/bjodah/local-aider) ·
[llama-server OpenAI-compat docs](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md)
