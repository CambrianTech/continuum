# Model parts

Status: proposed, 2026-11-20.

[Parts](README.md) · [Lifecycle](LIFECYCLE.md)

A model part is a part whose state is a loaded model: weights, a session, a runtime. Its lifecycle is
always the same: prepare loads it, suspend and shutdown release it, resume loads it again. So that
lifecycle is written once, and each model supplies only how it loads.

## The object owns its model

Today each audio backend keeps its model in a static global (`static WHISPER_RT:
ReloadableModel<...>` in `continuum-live/src/live/audio/stt/whisper.rs`, and one per backend). That
is a missing object (MODULE-ARCHITECTURE §9.3): state reached through a global instead of owned. A
model part owns its `ReloadableModel` as a field, and the statics are deleted.

## The generation

```rust
pub trait Loads: Part + Send + Sync {
    type Model: Send + Sync;
    type Failure: Send;
    fn model(&self) -> &ReloadableModel<Self::Model>;    // the field it owns
    fn load(&self) -> Result<Self::Model, Self::Failure>; // the leaf's only lifecycle code
}

// Written once: every Loads part is a Lifecycle part.
impl<T: Loads> Lifecycle for T { /* prepare = load_with(load); suspend/shutdown = unload;
                                    resume = load_with(load); an unload still held by
                                    in-flight callers is Pending until they drop it */ }
```

Whether a model part is loaded is its model's state (`model().is_loaded()`), read in one place by
registries and status commands.

## The families

| Family | Leaves | Kind's own verbs, at the kind's owner |
|---|---|---|
| Speech to text | Moonshine, Whisper, OpenAI realtime (a remote session), stub | transcribe, languages |
| Text to speech | Kokoro, Orpheus, Piper, Pocket, silence | synthesize, voices; a LoRA voice gene per resident |
| Voice activity | Silero, Silero raw, WebRTC, RMS threshold | detect |
| Inference adapters (next) | llama.cpp, Anthropic, OpenAI, remote peers | generate, health |

A part with no local model (a cloud session, a stub) still implements `Lifecycle`, with bodies that
open and close its session; it does not pretend to hold a model.

## Migration, in order

1. `ReloadableModel` moves to `continuum-base::part` beside `Lifecycle`, so the engine's adapters and
   the audio backends share it.
2. Each audio backend owns its model field and implements `Loads`; its statics, its `initialize`,
   `is_initialized` and `shutdown` are deleted; the speech and VAD traits keep only their own verbs.
3. The audio registries become `Children<&'static str, dyn Kind>`; the release loops in
   `AudioResourceLifecycle` and the voice module become one composite `shutdown`.
4. The inference adapters follow, each in its own change.

Each step deletes more than it adds, and lands with the validators of [Lifecycle](LIFECYCLE.md).
