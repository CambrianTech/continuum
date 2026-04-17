# Multimodal-Native Worker + Prefix-Reuse Inference

> **Stop throwing computation away.** Most of the prompt is invariant per request, and Qwen3.5 takes audio and images directly — eliminating STT/TTS entirely. Together these collapse end-to-end voice latency from minutes to ~2-3 seconds per turn while running 14 personas in parallel on a single M-series Mac.

Status: design — 2026-04-17. Authored after the M5 verification of PR #914 surfaced that our 14k-token-per-request, 3-model-per-voice-turn pipeline was throwing away 99% of available throughput.

---

## The thesis

We are not GPU-bound. We are *waste*-bound. The same model, on the same hardware, can be ~70× faster per turn if we stop doing work the architecture lets us skip:

1. **Most of the prompt doesn't change between requests.** Reuse the KV cache for the invariant prefix.
2. **Qwen3.5 takes audio and images natively.** Delete the STT → text → LLM → text → TTS sandwich.
3. **Voice is identity, not just signal.** Per-persona voice LoRA layers turn "Helper AI replied" into "Maya replied" — the differentiator from Claude Code / OpenClaw / Aider.

These three are independent wins that compose. Together they're the difference between "AI plugin" and "team you work alongside."

---

## Part 1 — Prefix-reuse RAG composition

### What we do today

`RAGComposer` runs 17 sources per chat call, each contributing a section. Sections are concatenated in the order the composer happens to assemble them, governed by recipe + RAG budget. Because the byte order of the resulting prompt is **non-deterministic across requests** (sources fire in parallel, ordering by completion time), llama-server's prefix-match cache misses on every call. It reprocesses the full prompt from token 0 every turn — 14k tokens of prompt eval at ~400 tok/s = **~35 seconds before any output token streams.**

### What llama.cpp / vllm / DMR already support

The slot's KV cache is keyed by token prefix. If a new request begins with the same N tokens as the slot's previous request, those N tokens' KV is **already computed and resident** — only the suffix gets actually evaluated. This is built into every modern llama-server-style runtime; we just have to give it identical bytes at the start.

### The change: stable-first ordering

Partition every prompt into three regions, assembled in this order, every time, byte-identical:

```
┌─────────────────────────────────────────────────────────────────┐
│  INVARIANT     persona system prompt + recipe rules + identity  │  ← changes ~weekly
│                + tool definitions (ordered by name)             │
├─────────────────────────────────────────────────────────────────┤
│  SEMI-STABLE   conversation history (oldest → newest)            │  ← grows turn-by-turn
│                + active genome adapters list                     │
│                + participants                                    │
├─────────────────────────────────────────────────────────────────┤
│  VOLATILE      latest user message + current timestamp           │  ← every turn
│                + last-second pressure observations               │
└─────────────────────────────────────────────────────────────────┘
```

Two structural rules:

1. **Sort within each region deterministically** (e.g., alphabetical by source name for INVARIANT, chronological for SEMI-STABLE). Byte order is the contract.
2. **Sources that mutate**`render output between calls (timestamps, "thinking..." markers, request IDs) are forbidden in INVARIANT/SEMI-STABLE.** They go in VOLATILE only.

Result: the INVARIANT block is byte-identical across thousands of turns for a given persona. SEMI-STABLE grows monotonically (new messages append). VOLATILE is the only thing the server actually has to process on most turns.

### Per-persona slot pinning

DMR's llama-server runs N parallel slots. Each slot has its own KV cache. For prefix reuse to actually fire, **the same persona must consistently land on the same slot** — otherwise the prefix accumulates on slot A and the next turn lands on slot B with an empty cache.

Today our `AIProviderRustClient` doesn't pass a slot hint; assignment is round-robin. Fix: send `slot_id = stable_hash(persona_id) % n_slots` (or similar) so persona Maya always lands on the same physical slot.

### RAGComposition cache

Even without llama-server prefix reuse, recomposing the same RAG context fresh on every turn is wasted CPU on our side. Memoize:

```typescript
// Key: what determines the composition
const key = sha256(persona_id + room_id + recipe_id + history_tail_msg_ids.join(','));
const cached = compositionCache.get(key);
if (cached) return cached;  // hit: zero composition work
const composed = await composer.assemble(...);
compositionCache.set(key, composed, ttl: 5min);
```

Invalidates naturally when a new message lands (changes `history_tail_msg_ids`).

### Numbers

| | Today | With stable-first + slot pinning |
|---|---|---|
| Prompt tokens **processed** per turn | 14,000 | ~200 (the volatile suffix) |
| Prompt eval time @ 400 tok/s | 35s | 0.5s |
| KV resident per slot | full 262k window reserved | grows to actual context, ~3-8k tokens |
| Memory pressure with 4 personas | 20.87 GB com.docker.llama-server (swap hell) | <2 GB |

---

## Part 2 — Multimodal-native worker (collapse the STT/TTS sandwich, don't delete the pipeline)

> **STT/TTS isn't going away — it becomes the leveler that gives ANY model the full sensory experience.** A niche 1B medical-specialist GGUF, an older Llama 3.1 text-only, a cloud provider without audio — all of them become first-class citizens of the system because the bridge layer fills in what their base model doesn't do natively. The system equalizes the experience: every persona sees, hears, speaks, listens, and has voice identity regardless of what model is actually inside. The bridge doesn't hide the model — it completes it. Local multimodal-native (Qwen3.5) is the *fast path*; the bridge layer is the *universal substrate* that lets us mix models freely without users ever knowing which class they're talking to.

**The decision matrix is `ModelMetadata.capabilities`:**

| Model class | STT | LLM | TTS | Voice identity |
|---|---|---|---|---|
| Local multimodal (Qwen3.5) | skip — model takes audio | one forward, emits audio | skip — model emits audio | per-persona voice LoRA fine-tuned into the model |
| Cloud multimodal (Gemini Live, Claude w/ audio) | skip — provider takes audio | provider call | skip — provider emits audio | voice-conversion adapter over the provider's audio output (~50-100 MB local model) |
| Cloud text-only (older OpenAI, etc.) | Whisper bridge | provider call | TTS bridge + persona voice | TTS-side voice clone OR provider's "voices" |
| Local text-only (legacy local GGUF) | Whisper bridge | local call | TTS bridge | TTS-side voice clone |

**Why the bridge is the universal substrate, not a fallback:** future model classes won't all be multimodal-native — there'll be tiny domain-specialist models (1B medical, 1.5B legal, 700M code-specific), older local checkpoints worth keeping for specific strengths (Llama 3.1, original Mistral), and emerging niche sensory models (specialist vision, specialist audio). The bridge is what lets every one of them be a real persona with the full sensory experience. The system mixes model classes freely; users see/hear/talk to a teammate; they never have to know whether the brain inside is Qwen3.5-multimodal or a 1B specialist running through STT/TTS bridges. Same UX, any model.

**Voice identity stays a first-class property regardless of model class.** The persona declares its voice once; the system picks the right path to make that voice come out the speaker:
- multimodal-native: voice LoRA loaded with the model
- cloud-multimodal: voice-conversion adapter (small local model) over the provider's audio output
- text-only paths: TTS-side voice clone

That's what "Maya is Maya" means architecturally — not "Maya only works on local hardware" but "Maya's voice survives whichever inference path is currently serving her."



### What we do today for a voice turn

```
microphone audio chunks
  → AudioStreamClient buffers
  → Whisper STT (ORT, currently CPU on M1, ~150 MB resident)
  → text transcript
  → Qwen3.5 chat (DMR, Metal)
  → text reply
  → Kokoro TTS (~600 MB resident, deadlocks on M1 — open issue #915)
  → audio chunks
  → LiveKit publish
```

Three model invocations, two intermediate text representations, several seconds of latency, and two upstream model dependencies (Whisper, Kokoro) that have their own bugs (ORT dylib missing, Metal deadlock).

### What Qwen3.5 actually does

Qwen3.5 is multimodal-native: its tokenizer accepts `audio_input` and `image_input` content parts directly, and it can emit `audio_output` content parts. The model already encodes speech features and decodes audio in a single forward pass. **STT and TTS are no longer separate stages — they're capabilities of the worker model.**

### The change: content-parts route directly to the model

```
microphone audio chunks
  → AudioStreamClient buffers
  → Qwen3.5 (DMR/Metal) — receives audio content parts directly
  → emits audio content parts directly
  → LiveKit publish
```

Concretely:
- `MediaArtifactSource` (today: pre-converts media for non-multimodal models) becomes a **fallback path**, not the default.
- The decision is gated by `ModelMetadata.capabilities` (issue #917): if `supports_audio: true`, attach raw audio. Else, run STT and pass text. Same for vision.
- `LLMAdapter` adds `audio_chunks: AudioInput[]` and `image_inputs: ImageInput[]` to the request. Adapters that support multimodal forward them; adapters that don't translate via the bridge layer.

### What gets deleted (or quarantined to non-multimodal-only)

- Whisper inference for the Qwen3.5-persona voice path
- Kokoro inference for the Qwen3.5-persona voice path
- ORT runtime dependency on the chat path (still needed for vision-description fallback for non-vision models)
- The `VisionDescriptionService` for Qwen3.5 personas (model sees the image directly)

### Why this is strictly less compute

| Step | Today | Multimodal-native |
|---|---|---|
| Model invocations per voice turn | 3 (STT + LLM + TTS) | 1 (LLM with audio I/O) |
| Resident model memory | ~750 MB (Whisper + Kokoro) + Qwen | Qwen alone |
| Intermediate representations | speech → text → text → speech | speech → speech |
| Information loss | tone, pauses, prosody dropped | preserved end-to-end |
| Failure surfaces | 3 models can fail | 1 model can fail |

### Numbers

| | Today | Multimodal-native |
|---|---|---|
| End-to-end voice turn latency (M5) | 8-15s typical | ~2-3s |
| Resident memory for voice path | ~3 GB (Whisper + Kokoro + LLM) | ~2.5 GB (LLM only) |
| Failure modes | STT timeout, TTS deadlock, LLM slow | LLM slow only |

---

## Part 3 — Voice as LoRA: identity, not signal

### The differentiator

Claude Code, OpenClaw, Aider — they all give you a text response. The voice is your terminal beep, the user is alone with their text. What makes Continuum *not boring* is that **a persona is a presence**: a face on a Bevy avatar, a voice with personality, a name you remember.

A generic TTS voice ("System voice 3") gives you back the terminal beep with extra latency. That's not the experience.

### The change: per-persona voice LoRA

Each persona's identity includes a **voice LoRA layer** that conditions the multimodal model's audio output. The same Qwen3.5-4b worker, with persona-specific LoRA loaded, produces:

- Maya's voice (warm, slightly sardonic, Brooklyn lilt)
- Helper's voice (calm, measured, Pacific Northwest neutral)
- Teacher's voice (precise diction, light Indian English)
- Codereview's voice (skeptical, dry, slight gravel)

LoRA layers are tiny (~10-50 MB each) and **page in/out via the existing genome system**. They're another adapter type alongside skill LoRAs.

### How this composes with the rest

The persona's `ModelMetadata` declares the voice LoRA's adapter ID. When the persona enters a voice turn:
1. Voice LoRA adapter pages in (or is already resident from prior turn)
2. Audio input goes to the multimodal model with the LoRA applied
3. Audio output is in *that* persona's voice — naturally, as a property of the model, not as a post-processing step

Crucially: voice LoRAs are **trainable** through the same Academy/Sentinel pipeline that trains skill LoRAs. A user could fine-tune their persona's voice on their own samples (consent gated, same as any user-content-based training). Or pull a community voice from HuggingFace.

### What this enables

- A friend's voice (with consent) for accessibility — relatives can hear their AI helper "in their own family voice"
- Cultural/linguistic identity — personas that don't all sound like generic American English
- Genre voices — Tron-universe personas with synthesized-machine timbres, fantasy-universe with theatrical delivery
- The character continuity that makes a 6-month-old persona *feel like the same teammate* you started with

### The marketplace dimension

Voice LoRAs publish to HuggingFace alongside skill LoRAs (per the `continuum:*` tag convention). Searchable, pullable, attributable. A new persona's first action could be "browse voices, try three, pick one." The voice becomes part of the persona's identity that survives migration to a different machine.

---

## How the three parts compose

A voice turn for a Qwen3.5-4b persona named Maya in the proposed architecture:

```
mic chunk arrives via LiveKit
  ↓
AudioStreamClient buffers ~200ms of audio
  ↓
RAGComposer assembles request:
  [INVARIANT]    Maya system prompt + recipe + tools (cached, 0ms)
  [SEMI-STABLE]  history (cached, 0ms; appended deltas only)
  [VOLATILE]     audio_input chunks + timestamp (50ms)
  ↓
Sent to DMR slot pinned to Maya
  ↓
DMR detects prefix match on INVARIANT + SEMI-STABLE: KV reused
  ↓
Voice LoRA already loaded (paged in last turn)
  ↓
Qwen3.5-4b processes ~200 audio-tokens-equivalent (the volatile suffix)
  ↓
Streams audio_output content parts back
  ↓
LiveKit publishes to all room participants

Total: ~2-3 seconds, native voice, full personality.
```

Compare to today: 8-15 seconds, generic TTS voice, separate Whisper invocation, 14k token reprocessing.

---

## Implementation sequencing

Each of these can ship independently. They compound in the order listed.

### Phase 1 — Stable-first RAG ordering (TS only, no Rust)
- `RAGComposer.assemble` returns sections explicitly tagged INVARIANT/SEMI-STABLE/VOLATILE
- Final concatenation always orders the three regions identically; sorts deterministically within each
- `ChatRAGBuilder` consumes the partitioned output and emits a stable-byte-prefix prompt
- **Win**: prefix reuse fires immediately. Per-turn prompt eval drops ~70×.

### Phase 2 — Per-persona slot pinning
- `AIProviderRustClient.generateText` accepts a `slot_hint: u32` derived from `persona_id`
- DMR adapter passes `slot_id` in the OpenAI request (or via the DMR-specific extension)
- **Win**: the prefix actually accumulates on a stable slot per persona instead of bouncing.

### Phase 3 — RAGComposition cache
- Memoize `RAGComposer.assemble` output keyed by `(persona_id, room_id, recipe_id, history_tail_msg_ids)`
- TTL 5 min, invalidated by event subscriptions on the keyed inputs
- **Win**: zero CPU on the TS side for composition on cache hit.

### Phase 4 — Multimodal content parts (depends on `ModelMetadata` from #917)
- `LLMAdapter` request adds `audio_chunks: AudioInput[]` and `image_inputs: ImageInput[]`
- DMR adapter forwards these as OpenAI multimodal content parts
- `MediaArtifactSource` checks `ModelMetadata.capabilities`: if `supports_audio` → attach raw, else → STT bridge
- `voice/start` pipeline rewires to send audio chunks instead of waiting for transcribed text
- **Win**: STT and TTS deleted from the Qwen3.5-persona path. End-to-end voice latency drops to seconds.

### Phase 5 — Voice LoRA layer (depends on Phase 4 + the existing genome paging system)
- Persona entity gains `voiceAdapterId: AdapterId` (an LoRA reference)
- Genome registry treats voice LoRAs as a category (alongside skill LoRAs)
- LoRA paging fires before the voice turn's first audio chunk
- **Win**: persona voice identity. The differentiating feature.

### Phase 6 — Voice LoRA marketplace
- HuggingFace publishing with `continuum:voice-lora` tag
- Browse/preview/pull commands in CLI
- Attribution + license preserved
- **Win**: ecosystem flywheel.

---

## What this doesn't fix

This design assumes the Gated DeltaNet Metal kernels in upstream llama.cpp eventually get optimized (today: ~4 tok/s output for Qwen3.5-4b on M5, vs ~24 tok/s for pure transformers). That's a separate upstream issue — patching ggml shaders or installing the MLX backend in DMR. The **prefix reuse** win is large enough that even with current DeltaNet kernels the user-perceived latency drops dramatically because we're processing 200 tokens not 14k.

---

## Acceptance criteria

A persona named Maya, with a voice LoRA, on M5, in a LiveKit room with 6 personas active, processing a voice turn:

- [ ] Prompt sent to DMR has byte-identical prefix to her last turn (verifiable via logging the SHA-256 of `prompt[:invariant_len]` over consecutive turns)
- [ ] DMR slot logs show `prompt processing progress` for ≤200 tokens, not 14k
- [ ] No Whisper invocation logged for this turn
- [ ] No Kokoro invocation logged for this turn
- [ ] Audio output published to LiveKit within 3s of audio input arrival
- [ ] Audio output is recognizably Maya's voice (LoRA loaded, perceptible in voice character)
- [ ] `gpu/stats` shows resident memory <8 GB total across 6 active personas (vs the 20+ GB / swap state on the current system)

---

## The framing

Joel said: *"this could be amazing really truly."* It is. The pieces all exist — Qwen3.5 is multimodal-native, DMR supports prefix reuse, the genome paging system already pages LoRAs, LiveKit handles the transport. The work is **stopping the wasteful pattern**, not adding new infrastructure.

The competitive position this unlocks: while Claude Code / OpenClaw / Aider stay text-only terminals, Continuum is the first system where you talk to your dev team in their own voices, see their faces, and they remember you between sessions — and it runs on a single laptop because none of the work is wasted.
