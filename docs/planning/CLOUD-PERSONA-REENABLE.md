# Cloud Persona Re-enable — readiness + the "far better adapter system" gaps

**Context (Joel, 2026-07-10):** re-enable the legacy-era cloud AIs as peers
("more like you"); keys exist (home-dir config.env / flash drive — will be
provided); legacy made ALL popular providers work; the good ones were
**deepseek, grok (xai), groq (price/speed), fireworks**; Google was broken.
Requirement: "a far better adapter designed system when we do cloud."

## What is ALREADY the better system (Rust core, today)

The legacy ELEGANT_ADAPTER_REFACTOR goals (small files, generics, no god
objects, providers-as-data) are implemented in the core:

- **Providers are DATA rows** (`model_registry/catalog.rs::ProviderSpec`):
  `base_url + api_key_env + auth + model_prefixes` — openai, deepseek,
  together, groq (with its `/openai` path prefix), fireworks, xai all present.
- **ONE parameterized adapter** (`ai/openai_adapter.rs::OpenAICompatibleAdapter
  ::new(config)`) serves every OpenAI-wire provider; Anthropic has its own
  native adapter (`ai/anthropic_adapter.rs`) — 2 adapters total, not N.
- **Typed URL boundary** (`ai/openai_endpoints.rs::OpenAiBase`) — the
  `/v1/v1` class of bug is unrepresentable.
- **Allocation is key-driven** (`persona/allocate`): one cloud persona per
  present `*_API_KEY` env. Add keys to `~/.continuum/config.env`
  ([[config-env-single-owner]]) → reboot → cloud citizens join the same
  rooms/kanban/mesh as the locals.
- **Tool dialect applies unchanged** (`cognition/tool_dialect`) — cloud models
  get the same conventional-name surface; they're the models we canNOT
  fine-tune, so meeting their trained dialect matters MOST there.
- **Key custody**: keys live in the adapter layer only
  ([[compute-lease-boundary]]); verified 2026-07-10 that no key value has
  ever reached a persona-visible surface.

## Gaps to close (the actual work when keys land)

1. **Google, fixed the modern way**: legacy fought Gemini's bespoke API. Today
   Gemini ships an OpenAI-compatible endpoint —
   `https://generativelanguage.googleapis.com/v1beta/openai` — so the fix is
   ONE more ProviderSpec row (`GOOGLE_API_KEY`, Bearer), zero custom adapter.
2. **Cost telemetry** (mine legacy `pricing.json` + COST-TRACKING-ARCHITECTURE):
   TurnMetrics already carries prompt/completion token counts; add per-model
   `price_in/price_out` to the Model row and a `turn.cost` probe. Cost is a
   first-class signal for the model-selection gas pedal
   ([[model-selection-is-a-dynamic-gas-pedal]]) and for groq/fireworks
   price-performance claims.
3. **Live model discovery per provider** (`/v1/models` at key-detect — the #74
   doctrine applied to cloud) so catalog rows stop pinning model ids that
   providers rotate.
4. **Rate-limit/backoff as adapter behavior** (429/529 handling with jittered
   retry + the probe trail) — cloud-only failure class the local lane never hit.
5. **Provider health on the ModelCatalog watch** (#78) so allocation skips a
   provider that is keyed but down — honest, never a silent fallback.

## Order

Keys land → verify allocate sees them → spawn ONE cloud citizen (deepseek or
groq first — the legacy-proven good ones) → mentor dynamics in the live room
(cross-capability curricula: strong-mind corrections become weak-mind training
pairs and vice versa) → then gaps 2–5 as the population grows.

## Doctrine (Joel, 2026-07-10) — cost is the ONLY new axis

Cloud = the same lane as local with one extra number: token price (local = 0).
Therefore:
- **Free-first is a hard invariant**: the system operates out of the box
  entirely free, no gating. Cloud is additive, never load-bearing.
- **Budgeting is financial and user-preferenced**: a spend ceiling on the
  Model row's price columns; the governor treats dollars like it treats VRAM.
- **Cloud's role is teacher/mentor**: paid tokens preferentially land on
  TEACHING moments (curriculum synthesis, review, correction of locals) —
  where they convert into permanent local weights and engrams. Every cloud
  turn should leave residue. The goal is making them obsolete; until then,
  extract knowledge and experience — distillation as exit strategy.
