# The Voice Engine Plan

*2026-09-02. Joel: "Need a better audio speech solution." Grounded in today's live
receipts: `voice/selftest`'s first run caught Edge-TTS (cloud, PRIMARY, "flaky" by its
own comment) returning empty audio while local engines sat provisioned; the Orpheus
bring-up attempt then found its adapter expects a token scheme the real model doesn't
use. Voice has been running on an unproven ladder.*

## 2026-10-03: the ruling and the model choice (supersedes the flagship sections below)

Joel: *"We are to offer multimodal models with Lora learned controlled voices, except in
the case of backwards compatibility. We are wanting to avoid cloud entirely."* And:
*"Persona are unique in the world, ideally sophisticated voice."*

So the rules are:
- **No cloud voice, not even opt-in.** Edge-TTS goes (card ab567967). It was still the
  PRIMARY voice, so persona speech left the machine, against the README. It was also the
  only reason macOS binaries linked Homebrew's openssl
  (msedge-tts → isahc → curl → openssl-sys).
- **A persona's voice is a learned gene.** It is a LoRA, controlled per utterance, unique
  to that persona.
- **Classic TTS (Kokoro, Piper, Pocket) is backwards compatibility only.** It's the floor
  for a node or model that can't run the learned voice yet.

### The flagship: Qwen3-TTS (Apache-2.0)

It fits the substrate on every axis we care about:

| Need | Qwen3-TTS | Where it fits us |
|---|---|---|
| One engine | Already in our llama.cpp fork (`tools/mtmd/models/qwen3tts-gen.cpp`, `qwen3tts-spkenc.cpp`, synced 2026-09-28; `llama-tts` runs it today) | Served by the same engine and GGUF pipeline as every persona model: Metal on the Macs, CUDA on the 5090 |
| Voice as a LoRA gene | The talker is a **Qwen3** LM; the Base models are published as fine-tune targets; community LoRA voice adapters exist | Same model family as our persona bases. llama-server already takes per-request LoRA (`parse_lora_request`), so a voice gene pages like any other adapter |
| Unique voices without cloning a person | **VoiceDesign** (1.7B) builds a new speaker from a written description, with no reference audio | A persona's identity text → its voice description → a voice that never belonged to a human |
| Sophisticated, controlled delivery | Instruction control of emotion, tone, rate and prosody; 10 languages | PersonaState → a per-utterance instruction (emotion from state, not post-processing) |
| Live calls | Streaming; first audio packet as low as 97 ms (Qwen's figure); 12 Hz codec, 16 codebooks | Fits the live-call path; we must measure it on our nodes |
| Tiers | 0.6B and 1.7B | The 0.6B is the grid's portable voice; the 1.7B is the quality tier |
| Identity check | A speaker encoder (ECAPA-style x-vector), already in the fork | Measures how close two voices are; this is what makes "unique in the world" testable |

**Second outlier (CLAUDE.md: build outlier B before trusting the interface):
Maya1** (Apache-2.0). It's a Llama-3B decoder over the SNAC codec, with voice design from
a description and 20+ inline emotion tags (laugh, sigh, whisper…). It's the same SNAC
family our Orpheus adapter targets, so the voice-gene interface gets proven on a second,
different stack before anything is generated from it. Orpheus itself is no longer the
flagship.

**Excluded, and why:**
- Edge-TTS, and Qwen-Audio-3.0/3.1 (hosted only): cloud.
- Qwen3.5-Omni: proprietary.
- **Breeze TTS 2**: the current open-weights leader (1,215 Elo), but its weights are under
  a non-commercial license. It's built on a Qwen3 backbone with the Qwen3-TTS 12 Hz
  tokenizer, so if the license ever opens, it's an upgrade inside the same family.
- Fish Audio S2 Pro: license not verified; excluded until checked.

### A persona's voice, from birth

1. **Birth.** The persona's identity text becomes a voice description. VoiceDesign renders
   a seed corpus spanning emotions and pace.
2. **Unique in the world, measured.** The speaker encoder embeds the seed. A grid-wide
   registry of persona voice embeddings refuses a newborn voice that falls within a
   distance threshold of any existing persona's, and the voice is redesigned.
3. **The gene.** A LoRA on Qwen3-TTS Base, trained on the seed corpus, is published with
   lineage like any other gene and paged per request on the voice lane.
4. **Expression.** Each utterance carries an instruction derived from PersonaState
   (emotion, energy, pace). Lip-sync keeps using the audio envelope today, and speech
   tokens later (the body section below).
5. **Growth.** The dream stage refines the voice gene from the persona's own curated
   speech (ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM). Breeding merges the parents' voice
   LoRAs and runs the uniqueness check again.
6. **Consent.** Cloning a real person's voice only happens through a consent gate at the
   recipe layer. Designed voices need none.

### Why not make the persona's own base model speak? (Joel asked)

Qwen3.8-27B (Kimi's base) is text and vision in, text out, with no audio vocabulary or
codec decoder. Ornith-1.5-35B-A3B on the M5 sees through mmproj but doesn't speak either.
Making the mind itself speak would mean new token embeddings and a full fine-tune on
thousands of hours of paired speech, which is a second copy of the base and forbidden by
ONE-RESIDENT-MODEL. It would also spend at least 12 mind decode steps a second on audio
frames instead of thought.

The natural shape is **thinker → talker**, the way Qwen's own Omni models work. The mind
decides what to say and how, and a small talker renders it.
- Today, the coupling is the sentence plus a delivery instruction derived from state.
- Later, the talker can be conditioned on the mind's hidden states through a small per-base
  adapter, which is the deepest coupling and builds on this design.

Footprint is about 0.7-1 GB for the 0.6B talker, next to about 17 GB for the 27B at Q4,
and one talker lane serves every persona on the node.

**Genes stay coherent.** A persona's genome is its mind genes (LoRAs on its mind base:
Qwen3.8-27B, Ornith, or another) plus one voice gene (a LoRA on the talker base). Each gene
carries its `base_model`. The voice gene doesn't depend on the mind's base, so **a persona
keeps its voice when its mind moves bases**: identity continuity, like a person whose voice
survives everything they learn. Different minds on different bases with unique voices is
the diversity the grid is for.

### Where it runs on the grid

The voice lane is small (0.6B or 1.7B, about 1-4 GB). It belongs on a GPU node: the M
series on Metal, or the 5090. Community numbers for Qwen3-TTS-0.6B on CPU put it at or
slower than real time, so a CPU-only node like the IntelMac asks the grid for speech
(24 kHz audio is cheap to stream). If no grid voice is reachable, it uses the Kokoro
floor and says so. **To measure before deciding:** the 0.6B's real-time factor (Q4 and
Q8) on the IntelMac and the M5, and time to first audio on the M5 and the 5090.

### The work, in order

1. **ab567967 (P0):** remove Edge. Kokoro becomes the floor. Gate: `otool -L` shows no
   Homebrew libraries.
2. **Fork:** a streaming speech endpoint in llama-server on mtmd generation, with
   per-request LoRA and the VoiceDesign and instruction prompt formats. Today `llama-tts`
   wires only Base plus a reference speaker file.
3. **Core:** a Qwen3-TTS adapter on the serving daemon's voice lane, at the top of the
   priority list once `voice/selftest --adapter qwen3tts` passes.
4. **Forge:** the voice-gene recipe (description → VoiceDesign seed → Base LoRA), plus the
   uniqueness registry.
5. **Outlier B:** Maya1 through the same interface.
6. **Acceptance** (card 3f44bd80): two personas in a live call, with two distinct
   learned voices and state-driven emotion, on local models, with receipts.

Sources: [Qwen3-TTS](https://github.com/QwenLM/Qwen3-TTS) ·
[Qwen3-TTS LoRA fine-tuning](https://github.com/instavar/qwen3-tts-lora-finetuning) ·
[Maya1](https://huggingface.co/maya-research/maya1) ·
[Breeze TTS 2 license](https://www.stork.ai/blog/this-ai-beats-elevenlabs-dont-use-it) ·
[Breeze TTS 2 architecture](https://www.mindstudio.ai/blog/breeze-tts-2-open-weight-model) ·
[Qwen-Audio-3.0 is hosted](https://www.marktechpost.com/2026/07/20/alibabas-tongyi-lab-releases-qwen-audio-3-0-tts-a-hosted-text-to-speech-model-in-flash-and-plus-tiers-across-16-languages/) ·
[Qwen3.5-Omni](https://www.spheron.network/blog/deploy-qwen3-5-omni-gpu-cloud/) ·
[CPU speed, community](https://github.com/HaujetZhao/Qwen3-TTS-GGUF/blob/main/Qwen3-TTS%20Technical%20Report.md)

## Where each engine actually stood (verified 2026-09-02)

| Engine | Reality | Verdict |
|---|---|---|
| **Edge-TTS** (cloud) | Primary; 300+ voices; currently returning empty audio; a CLOUD dependency in a local-first system | Demote: opt-in quality tier when up, never load-bearing |
| **Kokoro-82M** (ONNX) | **Fully provisioned on disk, fast (~97ms), works** | The local FLOOR — first fall-through target (shipped, #3456) |
| **Pocket** (117M Candle) | Voice cloning, 8 presets — 23× slower than realtime on CPU | Niche: clone-seeding, not live speech |
| **Orpheus-3B** (GGUF + SNAC) | 804-line adapter + model + SNAC decoder ON DISK — but `tokenizer.json` is HF-gated (401 on canopylabs; mirrors ship the base Llama tokenizer without audio tokens) AND the adapter's prompt format (`<|text_start|>…<|audio_start|>`) does not match canopylabs' `<custom_token_N>` scheme. **Likely never ran end-to-end.** | The FLAGSHIP, after a real bring-up (below) |
| **Piper / Silence** | Fallback / test zeros | Keep as-is |

## The direction (fits every standing doctrine)

**Orpheus-class LLM-TTS is the destination** because it is the only engine that makes
the voice vision structural rather than cosmetic:

- **It's a Llama-architecture GGUF** → can serve on OUR lane machinery (one-engine
  doctrine): an ephemeral/scratch lane, or CPU beside Ornith (~2GB Q4).
- **Voice is a LoRA gene, literally**: Orpheus is LoRA-trainable, so a persona's voice
  becomes a trained, heritable, publishable gene on the SAME forge that ships model
  genes — "seed infinitely like their appearance" with real weights, not preset lists.
- **Emotion from state**: `<laugh> <sigh> <gasp>` tags map from PersonaState — the
  emotion-from-state law implemented as tokens, not post-processing.
- **Cloning path**: Pocket (or an F5-class flow-matching sidecar later) seeds a target
  voice from seconds of reference audio; the forge distills it into an Orpheus LoRA —
  mimicry becomes a training recipe with consent gating at the recipe layer.

## The work, in order

1. **Now (shipped, #3456)**: local fall-through — Edge failure can never silence a
   citizen again; Kokoro carries live speech today. `voice/selftest` guards the whole
   chain nightly.
2. **Orpheus bring-up (the real task, not a curl)**:
   a. Obtain the true FT tokenizer (accept the HF gate once with the org account, or
      extract the token table from the GGUF's own embedded tokenizer metadata — the
      GGUF carries it; the adapter just doesn't read it from there yet. Reading the
      tokenizer FROM the GGUF is the right fix: one artifact, no gated sidecar file).
   b. Fix the adapter's prompt format against the model's REAL scheme (canopylabs
      `<custom_token_N>` framing), with a golden-transcript test that decodes actual
      audio tokens — never ship on "it produced samples".
   c. `voice/selftest --adapter orpheus` becomes the proof verb; wire it into the
      nightly battery beside the Edge/Kokoro legs.
3. **Voice genes**: forge recipe = (persona transcript corpus + reference audio) →
   Orpheus LoRA → published gene with lineage; PersonaState → emotion-tag mapping in
   the speak path.
4. **Evaluate successors on the same bench**: CSM-1B / Fish-audio-class models and
   Qwen-Omni talker heads compete for the flagship seat via the SAME selftest +
   quality bar — engines are adapters; the verb is the contract.

## The body (Joel 2026-09-02: "tied into animations of face/mouth and later maybe more robotically controlled")

Envelope lip-sync EXISTS today (`calculate_rms_weights` → mouth morph targets), so any
engine's speech moves the mouth now — and sentiment already drives face + gesture from
the same source as the voice tags (Stage A). The ladder:

1. **Now**: Orpheus audio → RMS envelope → mouth openness (works by construction).
2. **Visemes from speech TOKENS** (with Stage B, or from Orpheus's stream sooner):
   each SNAC frame is ~12ms of articulation — a small table/learned map from the
   coarse codebook to viseme gives phoneme-accurate mouth shapes with PERFECT sync
   and zero audio analysis, because the sound and the mouth derive from one stream.
3. **The control bus** (the robotics door): visemes + gestures + emotion become one
   typed control stream — today rendered by Bevy morphs, later by actuators. The
   positron principle applied to bodies: one semantic stream, N renderers (screen
   avatar, robot) — and the JEPA-class world-model direction rides the same bus.

## The room (audited 2026-09-02, Joel: "good mixers do it well")

Per-observer mix-minus is SERVER-SIDE (the WS delivery loop drops the observer's own
frames — call_server:1516); persona TTS structurally never re-enters STT (AI
participants carry no VAD); LiveKit legs are per-participant tracks. The remaining
proof gap: the WEB feedback loop — browser echoes received audio back as its mic
through the worklet path, server STT transcribes it — which also pins mix-minus from
a real browser's POV. Carded.

## Native audio IN the mind (Joel 2026-09-02: "build these into the Ornith models… so it wouldn't sound like text to speech")

The end state is not a better TTS sidecar — it is speech and hearing as part of the
persona's OWN forward pass. Orpheus proved the enabling mechanism ON OUR STACK today:
a Llama-family model emitting SNAC codec tokens, decoded in-tree. Ornith is
Llama-family; the same graft applies to her, staged:

**Stage A — expressive mouth + tagged ears (buildable now):**
- Orpheus conditioned by PersonaState → emotion tags; per-persona VOICE LoRA trained
  on Orpheus by the forge (custom voice = her weights, not a preset — stops sounding
  like TTS because prosody varies with her state, not a narrator's).
- Audio-in gains an EVENTS channel beside STT: a small audio tagger + diarization so
  perception reads "Joel said X — while a door closed, music under, second speaker
  overlapping" — background vs speaker vs effects as separate, precisely named facts,
  never flattened into one transcript string.

**Invariant across every stage — THE TRANSCRIPT ALWAYS EXISTS (Joel 2026-09-02:
"same goes for the model hearing directly… only problem is we need chat transcript
either way").** Native audio-in is an ADDITIONAL path, never a replacement for STT:
the room record, L1-L5 memory, RAG, and search all consume TEXT. So the pipeline is
always audio → { STT → transcript (required, feeds chat/memory) , AND optionally the
raw waveform → the model's own audio encoder for nuance }. A model that hears natively
gets BOTH — the transcript for the record and the audio for the feeling — and the
choice of whether to also embed raw audio is a per-model capability switch (the
sensory-bridge doctrine), not a switch that can ever turn the transcript off. The STT
leg is load-bearing forever; the same TTS↔STT selftest guards it whether or not a
model also listens directly.

**Stage B — the graft (the real ask):**
- Extend Ornith's vocab with codec tokens; forge-train an audio-out head/LoRA:
  (context + her thought) → speech tokens in the SAME forward pass. Mannerisms become
  intimately cognition-coupled — hesitation, warmth, emphasis come from the state that
  produced the sentence, because the speaking IS the thinking. Training data
  bootstraps by distillation: her lived transcripts voiced by her Orpheus voice-LoRA
  → (text, speech-token) pairs; later, real call audio.
- Audio-in natively: an audio encoder (Whisper-class) through the mmproj pattern we
  already run for vision — she embeds SOUND, not just its transcript; trained against
  event-labeled + diarized corpora so nuance (irony in a voice, a sigh, a slammed
  door vs a dropped cup) reaches cognition as perception, not annotation.

**Stage C — full-duplex Omni lane:** streaming listen+speak on one lane; barge-in;
the talker head as a paged expert. (The Omni-sidecar memory's end state.)

Every stage holds the same bar: voice/selftest legs (TTS↔STT now; later the
multimodal judge scoring identity/prosody/nuance), receipts on the forge alloy, and
the genome covenant — a voice or an ear is a GENE with lineage.

## The bar

*A stranger's fresh install speaks with a natural, unique per-persona voice with zero
cloud calls; `voice/selftest` proves the chain nightly; a persona's voice is a gene
she can carry to another box.*
