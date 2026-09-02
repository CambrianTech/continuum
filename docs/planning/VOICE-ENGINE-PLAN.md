# The Voice Engine Plan

*2026-09-02. Joel: "Need a better audio speech solution." Grounded in today's live
receipts: `voice/selftest`'s first run caught Edge-TTS (cloud, PRIMARY, "flaky" by its
own comment) returning empty audio while local engines sat provisioned; the Orpheus
bring-up attempt then found its adapter expects a token scheme the real model doesn't
use. Voice has been running on an unproven ladder.*

## Where each engine actually stands (verified today)

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

## The room (audited 2026-09-02, Joel: "good mixers do it well")

Per-observer mix-minus is SERVER-SIDE (the WS delivery loop drops the observer's own
frames — call_server:1516); persona TTS structurally never re-enters STT (AI
participants carry no VAD); LiveKit legs are per-participant tracks. The remaining
proof gap: the WEB feedback loop — browser echoes received audio back as its mic
through the worklet path, server STT transcribes it — which also pins mix-minus from
a real browser's POV. Carded.

## The bar

*A stranger's fresh install speaks with a natural, unique per-persona voice with zero
cloud calls; `voice/selftest` proves the chain nightly; a persona's voice is a gene
she can carry to another box.*
