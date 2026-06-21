# Unsloth Integration — Design Blueprint

**Status:** Design (worked out); implementation in slices (MCP server slice 1 landed — continuum #1680).
**Parent docs:** [AI-COMMAND-NAMESPACE](AI-COMMAND-NAMESPACE.md) · [INFERENCE-LANES-REALISTIC](INFERENCE-LANES-REALISTIC.md) · [ADAPTER-SYSTEM-ARCHITECTURE](ADAPTER-SYSTEM-ARCHITECTURE.md) · [GENOME-FOUNDRY-SENTINEL](GENOME-FOUNDRY-SENTINEL.md) · [FORGE-RECIPE-AS-ENTITY](FORGE-RECIPE-AS-ENTITY.md) · [SDK-API-SURFACE](SDK-API-SURFACE.md) · [GRID-ADDRESSING-AND-ROUTING](GRID-ADDRESSING-AND-ROUTING.md) · [SENSORY-TRANSPORT-SEPARATION](SENSORY-TRANSPORT-SEPARATION.md)

> All facts below are grounded in the live app (`http://127.0.0.1:8888`), unsloth's
> docs, and HTTP-header inspection — not assumption. Where a fact is unverified it
> is marked **(VERIFY)**.

---

## 1. Strategy: leverage the engine, keep the organism

Continuum's moat is the **organism** — persona cognition, the LoRA genome, the
airc grid, collaborative multi-modal living personas. Inference and training are
the **engine** — table-stakes, GPU-hard, undifferentiated plumbing we were
hand-coding (Candle → llama.cpp).

**Unsloth is that engine, done well.** It is llama.cpp underneath (the backend we
already chose), with a polished training/inference UX, mature LoRA pipelines, and
real optimization (≈2× faster, ~70% less VRAM). Leveraging it lets us spend our
time on the organism.

**Posture: bleeding edge.** Build off their latest and ride their optimization +
model-support velocity (audio, vision, and video as it matures) to stay ahead —
*but only through stable seams* (OpenAI-compatible `/v1`, ShareGPT JSONL, GGUF,
MCP), never their internal/beta APIs. That keeps "fast and ahead" from becoming
"coupled to a moving target."

**The prize:** fast, far-easier setup, containerized, multi-modal — best of all
worlds — by composing unsloth's engine with continuum's organism.

---

## 1.5 Decision record (2026-06-21): lean client, install-dependency, trim the fat

Joel committed the strategy to its endpoint: **"lean on unsloth till we get ourselves fast; then we can make our own adapters and phase out unsloth if we feel like it."** Concretely:

- **unsloth is a declared INSTALL DEPENDENCY** (already co-installed). continuum becomes a **lean client** that carries **no ML of its own** for the gateway path. One manual step ever — the `UNSLOTH_API_KEY` — then automatic (delegate model up/down/select to unsloth; provider keys live in unsloth's advanced settings).
- **Trim the fat (binary + install + native-link surface), in order:**
  1. **Embeddings → unsloth `/v1/embeddings`; DELETE `ort` + `fastembed` + the `libonnxruntime` dep** (`FastEmbedProvider`, `ModuleBackedEmbeddingProvider`, fastembed `EmbeddingModule`). ONNX is the wrong tech — heavy native dylib, panics on absence, install-friction. Deleting it shrinks the binary, removes an install step, and **deletes the startup ORT panic at its root** (don't patch it — remove the dep). Keep `LexicalEmbedder` (pure-Rust, zero-dep) as the degrade fallback.
  2. **Formalize unsloth as an install dependency** (installer ensures it + the key — autowire #33).
  3. **Inference → unsloth; retire the local `llama`/ggml/Metal compile** — the big trim (build time, binary size, the libllama double-free dance, the per-platform native linking all become unsloth's problem).
- **Everything is behind a trait** (`AIProviderAdapter`, `EmbeddingProvider` — both already exist). unsloth is **impl #1**. Our own optimized Candle/llama.cpp adapter is **impl #2, built when we choose**; "phase out unsloth" = delete an impl. **Reversible by construction:** the local ML work stays in git history + the tree, so removing it now is "move it behind a future adapter," not "throw it away" — it re-plugs as impl #2, already written.

### No feature is lost (audit)
| Capability | Owner after the trim |
|---|---|
| Inference, embeddings, LoRA training, base-model selection + lifecycle, **media/multimodal**, model sharing (HF / its registry), provider-key vault | **unsloth** (the engine) |
| Cognition / persona brain, memory + portable identity, airc mesh & coordination, the **foundry/forge** (sentinel-ai-contracted novel weights/arch), the **genome market** (P2P trust-scoped discovery + A/B economy) | **continuum** (the organism — never unsloth's job) |

**Foundry handoff (the one path worth naming):** the foundry *generates* a novel artifact (continuum-unique; unsloth fine-tunes, it doesn't forge) → **load it into unsloth via API** as a served model → serve via `/v1`, share via unsloth (HF/registry) **or** continuum's airc genome-exchange (trust-scope decides which). The genome flywheel still turns; continuum's market sits *on top of* unsloth's train+serve+share.

**Why now:** this is the headless-solid + lean-install push — it's what makes "easy to start, performant, alpha-this-week" real, and it directly fixes the failure mode that forced the redesign (too hard to install, not performant).

## 2. The decisive constraint: license

Unsloth is **dual-licensed**:

| Component | License | Implication |
|---|---|---|
| Core package (training/inference engine) | **Apache-2.0** | Use/integrate freely. |
| **Studio UI** | **AGPL-3.0** | Source-merging it into continuum (or one combined binary) pulls AGPL copyleft (incl. network-use) over the combined work. |

**Therefore: never source-merge the UI.** All integration is **arm's-length /
process-boundary** (network APIs, separate processes) — which is license-clean
*and* the right architecture anyway. This single fact decides the whole UI
question (§4).

---

## 3. What unsloth is (grounded)

A local desktop app on `127.0.0.1:8888` that:

- **Runs `llama-server` (llama.cpp) underneath** and auto-updates it (observed
  b9692→b9704, no restart). Backend = what continuum already targets.
- **Chat with tools**: Web search, Code (sandboxed Bash/Python), Chat-with-Files
  (built-in RAG — Hybrid retrieval, top-K passages), **MCP**, Projects.
- **Train / Recipes / Export** — a no-code LoRA fine-tuning pipeline driven by
  **Recipes**.
- **Hub** — model/recipe sharing network (HF-like).
- **Projects** → export chats as **ShareGPT JSONL** (canonical fine-tuning
  format), Raw JSONL, CSV; import chats.
- **Connections** (Settings) — connect **any OpenAI-compatible endpoint**
  (verified against `/v1/models`), plus OpenAI/Anthropic/Ollama/llama.cpp/vLLM,
  and **MCP servers**.
- **Multimodal** — supported model families include text, vision, audio, TTS,
  embeddings; chat accepts image (webp/png), audio, PDF attachments; "omni" GGUFs
  carry text+image+audio.
- Exposes its **own OpenAI-compatible API** (`unsloth studio -p 8888`,
  `-H 0.0.0.0` to serve other devices on the network → grid-addressable).
- **BETA** (badge observed) — treat as fast-moving; integrate via stable seams.

Two uncanny alignments with things we already designed:
- unsloth **Recipes** ≈ continuum **[ForgeRecipe](FORGE-RECIPE-AS-ENTITY.md)**.
- unsloth **Project → ShareGPT JSONL** ≈ continuum **genome layers trained on
  chats** ("trained on chats like this one").

---

## 3.5 Control surface — grounded API map (probed live 2026-06-21)

unsloth Studio exposes TWO HTTP surfaces on `:8888`: the OpenAI-compatible **`/v1`**
(serving) and the **`/api/*`** management backend ("Unsloth UI Backend — Training
and Model Management", spec at `/openapi.json`). Probed against the running
instance — **this is the complete delegation surface; every capability we delegate
has an endpoint:**

| Need | Endpoint(s) |
|---|---|
| Is a model loaded / what's loaded | `GET /api/inference/status` · `GET /api/inference/models` · `GET /v1/models` |
| **Load a model (keystone auto-load)** | `POST /api/inference/load` (+ `GET /api/inference/load-progress`) |
| Unload / bring a model down | `POST /api/inference/unload` |
| Is a model embedding-capable | `GET /api/models/check-embedding/{name}` |
| Is a model vision-capable (media) | `GET /api/models/check-vision/{name}` |
| Download a model (HF pull) | `POST /api/hub/download` (+ progress/cancel) |
| What's cached locally | `GET /api/hub/cached-models` · `/api/hub/cached-gguf` |
| Inference / Embeddings | `POST /v1/chat/completions` · `POST /v1/embeddings` |
| **Foundry handoff: forged checkpoint → serve/export** | `POST /api/export/load-checkpoint` · `POST /api/export/export/gguf` |
| Training data | `POST /api/datasets/upload` · `/api/data-recipe/*` |
| Health probe | `GET /api/health` |

**Implications:**
- **Auto-load (#24) is clean HTTP, not a CLI subprocess:** startup → `GET /api/inference/status`; if empty → `POST /api/inference/load {configured model}` → poll `load-progress` → `GET /api/health`. Reliable + hands-off after the key — the reliable-startup keystone.
- **Embeddings (#40):** verify an embedding-capable model via `check-embedding`, ensure it's loaded, embed via `/v1/embeddings`.
- **Foundry handoff is real + endpoint-backed:** forge → `POST /api/export/load-checkpoint` → serve/`export/gguf` — the §1.5 handoff made concrete.
- **Multimodal is UNIFIED through the chat-completion request (grounded; multimodal is non-negotiable):** there is no separate `/v1/audio/transcriptions` because vision-in and audio-in (STT) ride as OpenAI-style multimodal **`content` parts** (`image_url`, `input_audio`) on `/v1/chat/completions`; speech-out is `POST /v1/audio/generate` (which itself takes a `ChatCompletionRequest`). Per-model capability is gated by the load-response flags `is_vision` / `is_audio` / `has_audio_input` / `is_diffusion`. So the adapter handles ALL modalities through **one chat seam carrying multimodal content** + the right loaded model — not N endpoints. **Avatar video calls** = unsloth multimodal (see/hear/speak) ⊕ continuum's own live layer (`LiveKitAgentManager` + Bevy avatar renderer in `main.rs` — WebRTC/render, never unsloth's job). Both halves are in tree.
- **Live state (2026-06-21):** unsloth starts **EMPTY** (`/v1/models` → `[]`), so nothing serves until a model is loaded — THE prerequisite for "peers online" (the auto-load keystone #24). **VERIFIED:** a manual `POST /api/inference/load {model_path}` of a local GGUF (`~/.continuum/genome/models/qwen2.5-0.5b-instruct`) loads instantly and `/v1/chat/completions` then **replies** — the persona reasoning chain works end-to-end through unsloth on this box. (Embeddings need the model started with `--embeddings` via the load schema's `llama_extra_args`, or a dedicated embedding model — the #40 detail.)

## 4. UIs stay separate (no merge, either direction)

Verified from the live app:
- `X-Frame-Options: DENY` + CSP `frame-ancestors 'none'` → **cannot be iframed**
  into a continuum page.
- CSP `script-src 'self'`, `connect-src 'self' + huggingface` → can't inject our
  JS or have their page call our API.
- No documented widget/plugin/extension surface.
- (And §2: the UI is AGPL — embedding would be copyleft-fraught regardless.)

**Conclusion — UI-merge is out both ways.** Instead:
- **continuum keeps its own UI** (~30 existing widgets + CSS: chat, persona-brain,
  settings-nav, voice-bar, continuum-metrics, web-view, factory, …). This is the
  rich/dynamic/real-time UI, including WebRTC video + avatars.
- **unsloth keeps its UI**, launched as **its own window/tab** from a continuum
  Settings link (e.g. "Model & Training" → opens `127.0.0.1:8888`). Launching ≠
  embedding; this is fully supported and AGPL-isolated.
- The synergy is **brain-level and headless** (§5), not UI.

---

## 5. The three brain seams (all headless, license-clean)

All three cross a process/network boundary (Apache-core or network API), so all
three are Node-free and AGPL-clean. **MCP/command path stays headless Rust** —
TS is client-only; no Node where it isn't required.

### Seam 1 — continuum AS an MCP server  *(slice 1 landed: #1680)*
unsloth's chat calls continuum's genome / cognition / airc-rooms / grid as MCP
tools. Implemented as a **Rust-native MCP server** that is itself a *client* of
the headless core ([persona-is-a-client]):
- `tools/list` → `execute("mcp/list-tools")` — `modules/mcp.rs` stays the single
  source of truth (commands ARE tools).
- `tools/call` → map MCP tool name → command path → `execute(command, arguments)`.
- **Strongly typed protocol** (`mcp_protocol.rs`): every JSON-RPC/MCP message is a
  serde struct; `Value` only at the two dynamic seams (tool `arguments`, command
  result). This **replaces `src/mcp-server.ts`** (871 lines of Node + a duplicate
  tool catalog) — removing a forbidden Node dependency and a compression violation.
- **Transport (slice 2):** stdio + HTTP/SSE bin pumping bytes into
  `McpServer::handle_message`, with `CommandDispatch` wired to a
  `continuum_client::Connection` over the core (gated like any caller). Then
  `src/mcp-server.ts` is retired.

### Seam 2 — continuum AS a Custom OpenAI-compatible provider
continuum exposes `/v1/models` + `/v1/chat/completions`; unsloth adds it as a
Connection and **chats *through* a continuum persona** (full cognition + genome)
as if it were a model. The persona's turn pipeline produces the completion.

### Seam 3 — unsloth AS a continuum inference backend  *(replaces Candle/raw llama.cpp)*
continuum's `AIProviderAdapter` points at unsloth's OpenAI-compatible `/v1`
(local `llama-server` at `:8080`, or Studio's own endpoint). unsloth owns the
llama.cpp lifecycle (GGUF, quant, auto-update, training); continuum orchestrates.
**Keep it behind the adapter boundary** — unsloth is the *preferred* local
backend, **not a hard dependency**: continuum must still boot with another
provider (`solve-for-public-users`, reversibility, beta-risk insulation).

---

## 6. Media / multimodal (the worked-out hard part)

**The worry:** if inference is across the wire, does image/audio/video input into
multimodal models work?

**Resolution — two-path capability negotiation, with a local safety net so we
never *bet* on the wire.** The `AIProviderAdapter` negotiates, per
(backend, model): *does this endpoint accept image/audio content parts?*

### Path A — native pass-through (when the model + backend support it)
- **Image: verified.** llama-server with the multimodal projector (`mmproj`) +
  a vision GGUF accepts **base64 images via OpenAI content parts**
  (`{type:"image_url", image_url:{url:"data:image/png;base64,…"}}`). This is the
  standard wire shape; unsloth supports it.
- **Audio: (VERIFY)** — "omni" GGUFs carry audio and unsloth chat accepts audio
  attachments, but the audio-over-`/v1` content-part path is less standardized
  than image. To be tested hands-on in slice 2.
- **Video:** not yet prevalent in local GGUFs; when it matures, the same
  content-part mechanism + capability negotiation extends to it for free. This is
  a primary reason to ride unsloth's model-support velocity (bleeding edge).

### Path B — local sensory bridge (model-agnostic, always works)
This is **already continuum's architecture** ([SENSORY-TRANSPORT-SEPARATION](SENSORY-TRANSPORT-SEPARATION.md),
`VisionDescriptionService`, STT): when the model/backend can't take raw media,
continuum **preprocesses locally → text** (image → description, audio → STT
transcript) and sends text. No model dependency.

**Why Path B is often *preferable* even when Path A works:**
- Wire cost: a base64 frame is large; for a remote GPU node, shipping a text
  description is far lighter than megabytes per turn.
- Privacy + caching: a content-addressed description (already cached) beats
  re-sending raw bytes.
- The adapter chooses per-task: raw pixels when the task genuinely needs them,
  description otherwise.

**Default:** adapter defaults to **Path B (local bridge → text)** — guaranteed,
wire-light — and opts into **Path A** when capability negotiation confirms the
model + backend accept the media content parts.

### What never crosses the unsloth wire
The **rich real-time stuff** — WebRTC video, avatars, live voice — is continuum's
**Live Engine, local** (the M1 avatar video chats). Only *composed prompt →
tokens* is leased to unsloth (the **compute-lease boundary**: brain + tool
execution stay local; only token generation is remote). So avatar/video richness
is independent of unsloth's multimodal support entirely.

---

## 7. Docker + grid: light personas, one shared engine

Leveraging unsloth **simplifies the fleet**:
- **continuum persona containers go light** — just the Rust core (cognition,
  genome orchestration, airc) + an HTTP client. **No CUDA / engine / model weights
  baked in.** N of these spin up cheaply.
- **One shared heavy GPU node** (unsloth's container) does inference + training.
  The GPU-hard image doesn't vanish — it *moves to unsloth*, shared, not
  replicated per persona.
- Because unsloth binds `0.0.0.0`, the **unsloth node is grid-addressable**: it
  slots into the airc grid as a compute peer. A light continuum persona on any
  node **leases `ai/generate` across the grid** from the unsloth GPU node — the
  exact model in [INFERENCE-LANES-REALISTIC](INFERENCE-LANES-REALISTIC.md) +
  [GRID-ADDRESSING-AND-ROUTING](GRID-ADDRESSING-AND-ROUTING.md). "Leverage unsloth
  across the grid" = N light persona nodes, M shared unsloth engine nodes,
  negotiated over airc.

---

## 8. Training: adopt the standards, converge the recipes

- **Data:** unsloth Project → **ShareGPT JSONL** is continuum's genome training
  data. A persona's chats (incl. collaborative airc rooms) export to ShareGPT and
  train a genome layer — "trained on chats like this one," now with a built-in
  export path.
- **Recipes:** unsloth **Recipes** ≈ continuum **[ForgeRecipe](FORGE-RECIPE-AS-ENTITY.md)**.
  Converge them: continuum's foundry *orchestrates* (what/when/which layer),
  unsloth *executes* the train. We keep the genome intelligence; hand off the
  GPU mechanics.
- **Sharing:** genome/adapter sharing rides unsloth **Hub** or **HF** —
  interop, not bespoke lock-in.

---

## 9. Dropped: Hermes

Hermes is just another agent on the same unsloth/llama.cpp stack (unsloth ships a
"Hermes Agent" integration doc). Target unsloth directly — same backend, fewer
layers.

---

## 10. Build plan (slices)

Built incrementally, validated each step; collaboratively over airc where
personas can help (leveraging unsloth across the grid for their own inference).

| Slice | Scope | State |
|---|---|---|
| 1. MCP protocol handler | `mcp_protocol.rs` — typed JSON-RPC, tools/list+call, headless | **Landed (#1680)** |
| 2. MCP transport bin | stdio + HTTP/SSE; `CommandDispatch` → `Connection`; retire `src/mcp-server.ts`; live-validate vs unsloth + Claude Code | Next |
| 3. unsloth inference adapter | `AIProviderAdapter` over `/v1` + capability negotiation (§6 Path A/B); adapter boundary (not hard dep) | — |
| 4. OpenAI-compatible provider endpoint | continuum `/v1/models` + `/v1/chat/completions` → persona-as-model | — |
| 5. Docker fleet | light persona image (no CUDA) + shared unsloth GPU node, grid-leased `ai/generate` | — |
| 6. Training convergence | ShareGPT export ↔ genome; Recipes ↔ ForgeRecipe; Hub/HF sharing | — |
| 7. Settings UI launch | continuum Settings link → opens unsloth UI (own window, AGPL-isolated) | — |

## 11. Open / to-verify
- **(VERIFY)** audio-over-`/v1` content-part support (slice 2/3 hands-on); until
  then, audio uses Path B (local STT).
- **(VERIFY)** image base64 passthrough end-to-end through unsloth's `/v1` to a
  vision GGUF.
- Capability-negotiation handshake shape (how the adapter learns a model's media
  caps — `/v1/models` metadata vs. probe vs. config).
- unsloth headless-in-Docker ergonomics for the shared GPU node.
- Recipe ↔ ForgeRecipe field mapping (concrete schema convergence).
