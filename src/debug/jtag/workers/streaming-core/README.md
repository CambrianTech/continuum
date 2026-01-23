# Streaming Core

Universal streaming backbone for AI communication.

## Architecture

**Everything is streaming** - same infrastructure, different timescales:
- Voice: 20ms frames
- Images: 2-30 seconds
- Video: 30-300 seconds
- Training: Hours

### Core Primitives

| Primitive | Description |
|-----------|-------------|
| **Handle** | Universal correlation ID (UUIDv4) - same as entity IDs |
| **Frame** | Data unit (Audio, Video, Text, Image) |
| **RingBuffer** | Lock-free queue with backpressure |
| **Event** | Handle-correlated status updates |

### Pipeline Model

```
InputAdapter -> [Stage1] -> [Stage2] -> ... -> OutputAdapter
     ↓              ↓           ↓                   ↓
  EventBus ← ← ← Events (Started, Progress, FrameReady, Completed)
```

### Zero-Copy Design

- Ring buffers hold data, pass SlotRef (8 bytes)
- GPU textures stay on GPU, pass texture ID
- Only copy at boundaries (encode/decode)

## Usage

```rust
use streaming_core::{Pipeline, PipelineBuilder, EventBus, StreamEvent};
use std::sync::Arc;

// Create event bus
let event_bus = Arc::new(EventBus::new(1024));

// Build voice chat pipeline: Mic -> VAD -> STT -> LLM -> TTS -> Speaker
let mut pipeline = PipelineBuilder::new(event_bus.clone())
    .voice_chat();

// Subscribe to events
let mut events = event_bus.subscribe_handle(pipeline.handle());

// Start pipeline (returns handle immediately)
let handle = pipeline.start().await?;

// Process events
while let Ok(event) = events.recv().await {
    match event {
        StreamEvent::FrameReady { .. } => { /* process frame */ }
        StreamEvent::Completed { .. } => break,
        _ => {}
    }
}
```

## Pre-built Pipelines

```rust
// Voice chat: Mic -> VAD -> STT -> LLM -> TTS -> Speaker
PipelineBuilder::new(event_bus).voice_chat()

// IVR: Twilio -> VAD -> STT -> LLM -> TTS -> Twilio
PipelineBuilder::new(event_bus).ivr(stream_sid)

// Image generation: Text prompt -> SDXL/Flux -> Image
PipelineBuilder::new(event_bus).image_gen()

// Video generation: Text prompt -> Mochi/CogVideoX -> Video
PipelineBuilder::new(event_bus).video_gen()

// Avatar: Audio -> LivePortrait/SadTalker -> Video
PipelineBuilder::new(event_bus).avatar()
```

## Stubbed Components (TODO)

### Adapters
- `CpalMicrophoneAdapter` - Local mic via cpal
- `CpalSpeakerAdapter` - Local speaker via cpal
- `TwilioMediaAdapter` - Twilio Media Streams
- `WebRtcInputAdapter` - WebRTC peer
- `WebRtcOutputAdapter` - WebRTC track

### Stages
- `VadStage` - Voice Activity Detection (Silero VAD)
- `SttStage` - Speech-to-Text (Whisper)
- `TtsStage` - Text-to-Speech (XTTS/MeloTTS)
- `LlmStage` - LLM inference (Ollama/Candle)
- `ImageGenStage` - Image generation (SDXL/Flux)
- `VideoGenStage` - Video generation (Mochi/CogVideoX)
- `AvatarStage` - Avatar animation (LivePortrait/SadTalker)

## Testing

```bash
cargo test -p streaming-core
```

## Building

```bash
cargo build -p streaming-core
```

## Proto (gRPC)

Proto definitions in `proto/streaming.proto`. Requires protoc for compilation.

```bash
# Install protoc (macOS)
brew install protobuf

# Build with proto
cargo build -p streaming-core
```

## Architecture Document

See `docs/architecture/STREAMING-BACKBONE-ARCHITECTURE.md` for complete details.
