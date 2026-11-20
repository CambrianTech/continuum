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

**Fit beats leaderboard rank.** Qwen3-TTS is today's best fit, not a permanent choice;
any model can take the seat by fitting better. Score a candidate on these criteria, in
order. The first three are gates:
1. **Local and open:** weights we can run and ship (a commercial-use license); no hosted API.
2. **One engine:** it runs in our llama.cpp fork (GGUF), or can be added there for less
   than the cost of a second runtime.
3. **Voice as a gene:** the voice can be a LoRA (or an equally small, base-bound
   artifact) that pages per request.
4. **Unique without cloning a person:** voice design from a description, or a speaker
   space we can sample and measure.
5. **Controlled delivery:** per-utterance emotion, pace and prosody, driven by state.
6. **Live:** streaming, and time to first audio that a call can carry.
7. **Footprint:** small enough that a node holds the mind plus the voice lane; a CPU tier
   for weak nodes, or a grid stream.
8. **Grid-portable:** the same gene works on Metal, CUDA and CPU.
9. **Quantizes well:** quality holds at Q4/Q8 (measured on `voice/selftest`, not
   assumed), ideally quantized by our own forge with the alloy receipt. A well-quantized
   larger model can beat a small unquantized one at the same footprint.
10. **Newest first:** the field moves monthly (most of the candidates above shipped in
    2026), so re-scan new open releases against this list before each voice milestone,
    and take the seat on the bench.

Breeze TTS 2 leads the leaderboard and fails gate 1. That is what "fit beats rank" means.

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
   **The registry is grid state, so it needs an owner (Fable, #4697 review).** An
   embedding is part of the voice gene's published record (lineage, alloy), so the
   registry is a projection of the published genes, not a separate store. Each node keeps
   its view; a birth checks against that view and records the view's high-water mark.
   A partitioned node can still give birth, but the voice is marked **provisional**
   until its node rejoins and the check is replayed against the merged view. A collision
   found then is resolved by redesigning the YOUNGER voice. Nothing is evicted: published
   genes are permanent, and a retired persona's voice stays reserved.
3. **The gene.** A LoRA on Qwen3-TTS Base, trained on the seed corpus, is published with
   lineage like any other gene and paged per request on the voice lane.
4. **Expression.** Each utterance carries an instruction derived from PersonaState
   (emotion, energy, pace) **and from the mind's own epistemic state: confidence, hedging,
   pressure.** A voice carries how a mind handles pressure and uncertainty, not just its
   timbre: the pause before a hard claim, no rising tone when it's unsure, warmth that
   shows in timing rather than pitch. (Fable, #4697: "the thing it should learn from her
   own transcripts is that rhythm.") Lip-sync keeps using the audio envelope today, and speech
   tokens later (the body section below).
5. **Growth.** The dream stage refines the voice gene from the persona's own curated
   speech (ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM). The training signal is **rhythm and
   prosody paired with what the mind was doing when it spoke**: transcript, confidence and
   state, not the timbre alone, so the voice grows with the mind's character. Breeding merges the parents' voice
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

### Who we size for (Joel, 2026-10-03)

*"We are not the ai hobbiest or tech community in general. I go for common gamer and dev
setups."* Our varied grid is for testing. Defaults are sized for what users already own,
with a designed plan for the weak floor (the IntelMac, a 1080 Ti).

Weight sizes below are real Hugging Face file sizes (2026-10-03); **context (KV cache)
comes on top**, so a tier is "fits" only with headroom. Voice = Qwen3-TTS 1.7B at Q4_K_M
(1.04 GB) + its codec (0.45 GB at Q8) ≈ **1.5 GB**. The 0.6B has no published GGUF yet;
our forge converts it, and its size gets measured then.

| Tier (common hardware) | Mind (weights) | + vision | + voice | Verdict |
|---|---|---|---|---|
| **8 GB** (RTX 3060 Ti / 4060 / laptop GPUs) | Ornith-1.5-9B Q4_K_M 5.78 GB | 0.92 GB | 1.5 GB | Too tight with context. Mind + vision local; voice from the grid, or the 0.6B once measured |
| **11 GB** (GTX 1080 Ti, Pascal: no tensor cores) | Ornith-1.5-9B Q4_K_M 5.78 GB | 0.92 GB | 1.5 GB | ~8.2 GB + context fits; slower kernels, measure |
| **12 GB** (RTX 3060 12 GB / 4070 / 5070, Arc B580) | Ornith-1.5-9B Q4/Q5 (5.8-6.6 GB) | 0.92 GB | 1.5 GB | Fits: the typical gamer persona node |
| **16 GB GPU** (4060 Ti 16 / 4070 Ti S / 4080 / 5070 Ti / 5080) | Ornith-1.5-9B Q8 9.79 GB | 0.92 GB | 1.5 GB | Fits at high quality; or Ornith-35B-A3B with experts offloaded to RAM |
| **16-24 GB Mac** (dev MacBooks, Mac mini) | Ornith-1.5-9B Q4/Q5 | 0.92 GB | 1.5 GB | Fits within macOS's GPU share of unified memory |
| **24 GB** (RTX 3090 / 4090, RX 7900 XTX) | Qwen3.8-27B Q4 ≈ 17 GB, or Ornith-35B-A3B Q4 21.71 GB | (27B is VL) / 0.9 GB | 1.5 GB | 27B + voice fits; 35B-A3B + voice needs a few experts offloaded |
| **32 GB+** (5090; 48-128 GB unified: M Pro/Max, Strix Halo, DGX Spark) | Ornith-35B-A3B Q4 21.71 GB or 27B | ✓ | 1.5 GB | Full node: several minds and one voice lane serving all |
| **CPU only** (IntelMac) | client-first | — | from the grid | Kokoro floor if no grid voice is reachable; never crash |

**The budget on 8-16 GB is the KV cache, not the weights (Fable, #4697).** All three
bases are hybrid models: only one layer in four is full attention and carries per-token
KV, while the linear-attention (GatedDeltaNet) layers keep a small fixed state per slot.
Figures from each model's config.json (2026-10-04), f16 K+V:

| Base | Full-attention layers × KV heads × head dim | KV per token | 32k ctx per slot |
|---|---|---|---|
| Qwen3.8-27B | 16 × 4 × 256 | 64 KiB | ~2.1 GB |
| Ornith-1.5-9B | 8 × 4 × 256 | 32 KiB | ~1.0 GB |
| Ornith-1.5-35B-A3B | 10 × 2 × 256 | 20 KiB | ~0.66 GB |

Check: the M5's 27B lane (`-c 102144 --parallel 3`, 34,048 ctx per slot) holds about
6.2 GB of KV, not the 25 GB a dense 64×8×128 model would. q8 KV halves these. On a 12 GB
card, Ornith-9B Q4 + vision + voice (~8.2 GB) leaves about 2.5 GB after runtime overhead:
about 80k tokens of f16 KV, i.e. two 32k slots (or four at q8). **The governor sizes
slots × ctx per tier from these per-token figures, and counts the resident voice lane
against the same budget.**

Gaps to close by measurement: the 0.6B talker's size and quality at Q4/Q8; real-time
speed of the voice on 8-12 GB cards and on the 1080 Ti; KV headroom per tier at our
context sizes.

### Where it runs on the grid

The voice lane is small (0.6B or 1.7B, about 1-4 GB). It belongs on a GPU node. The
grid's machines (Joel, 2026-10-03) are the 5090 and the 3090 (CUDA), the M5 Pros and an
M1 (Metal), the IntelMac (CPU) and BIGGIEDESK (Windows). The 3090 (24 GB) and the M1 can
each carry a voice lane beside a smaller mind, or serve voice for the grid, so the 5090's
and the M5s' memory stays with the big minds. Which machine serves voice is a placement
decision for the grid governor, measured, not fixed here. Community numbers for Qwen3-TTS-0.6B on CPU put it at or
slower than real time, so a CPU-only node like the IntelMac asks the grid for speech
(24 kHz audio is cheap to stream). If no grid voice is reachable, it uses the Kokoro
floor and says so. **To measure before deciding:** the 0.6B's real-time factor (Q4 and
Q8) on the IntelMac and the M5, and time to first audio on the M5 and the 5090.

### The work, in order

1. **ab567967 (P0):** remove Edge. Kokoro becomes the floor. Gate: `otool -L` shows no
   Homebrew libraries.
2. **Outlier A, the core adapter:** a Qwen3-TTS adapter against the fork's EXISTING
   non-streaming `llama-tts` path (Base + reference speaker, plus a LoRA), proven by
   `voice/selftest --adapter qwen3tts`.
3. **Outlier B:** Maya1 (Llama + SNAC, a different stack) through the same interface.
   Only once both fit without forcing is the interface trusted (CLAUDE.md outlier rule;
   Fable, #4697: otherwise it gets designed around one engine's streaming shape).
4. **Fork:** the streaming speech endpoint in llama-server on mtmd generation, with
   per-request LoRA and the VoiceDesign and instruction prompt formats, built to the
   proven interface.
5. **Forge:** the voice-gene recipe (description → VoiceDesign seed → Base LoRA), plus the
   uniqueness registry as described above.
6. **Acceptance** (card 3f44bd80): two personas in a live call, with two distinct
   learned voices and state-driven emotion, on local models, with receipts. Then the
   learned voice takes the top of the priority list.

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
