# Live Call Architecture -- Game Engine Philosophy for Real-Time Calls

> Handles not copies, command buffers not RPC, zero-copy pipelines, resource pools not per-frame allocation. Signaling and data transport are separate systems. This is how game engines work, and it's how live calls must work.

**Parent:** [Live](README.md)

## Philosophy: Game Engine, Not Web Dev

**Core principles (non-negotiable):**
1. **Handles, not copies** - Pass IDs to the deepest layer, never copy audio/video buffers
2. **Command buffers** - Batch operations, submit atomically, execute on dedicated thread
3. **Zero-copy pipeline** - Audio/video data stays in native memory, JS only holds handles
4. **Resource pools** - Pre-allocate, reuse, never allocate per-frame
5. **Separation of concerns** - Signaling vs data transport are different systems

Reference: [bgfx](https://github.com/bkaradzic/bgfx) handle architecture

---

## Handle Types

```rust
// All handles are u32 indices into typed pools
// Generation bits prevent use-after-free

pub struct AudioStreamHandle(u32);   // Live audio input/output stream
pub struct AudioMixHandle(u32);      // Mixed output for a participant
pub struct VideoFrameHandle(u32);    // Video frame in GPU/shared memory
pub struct CallHandle(u32);          // Active call instance
pub struct ParticipantHandle(u32);   // Participant in a call
pub struct TransportHandle(u32);     // WebRTC/WebSocket connection
```

**Handle lifecycle:**
```
create() → Handle    // Allocate from pool, return handle
use(Handle)          // Operations reference by handle
destroy(Handle)      // Return to pool, increment generation
```

---

## Architecture Layers

```
┌─────────────────────────────────────────────────────────────────┐
│                        APPLICATION LAYER                         │
│  Commands: live/start, live/join, live/mute, etc.               │
│  Widget: LiveWidget (UI only, no audio processing)              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼ Commands (signaling only)
┌─────────────────────────────────────────────────────────────────┐
│                      CALL MANAGER (TypeScript)                   │
│  - Call state machine                                           │
│  - Participant tracking                                         │
│  - Handle bookkeeping (does NOT touch audio/video data)         │
│  - Routes signaling between browser ↔ server ↔ peers           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼ Handles + Commands
┌─────────────────────────────────────────────────────────────────┐
│                   STREAMING CORE (Rust Worker)                   │
│  - Owns ALL audio/video buffers (zero JS copies)                │
│  - Audio capture → AudioStreamHandle                            │
│  - Audio mixing → AudioMixHandle                                │
│  - Video encoding/decoding                                      │
│  - TTS injection (AI voice)                                     │
│  - Command buffer execution                                     │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼ Raw packets
┌─────────────────────────────────────────────────────────────────┐
│                      TRANSPORT LAYER                             │
│  - WebRTC DataChannel (P2P when possible)                       │
│  - WebSocket fallback (server relay)                            │
│  - Handles only carry packets, never decode                     │
└─────────────────────────────────────────────────────────────────┘
```

---

## Audio Pipeline (Zero-Copy)

### Browser → Rust (Mic Capture)

```
┌──────────────┐    SharedArrayBuffer    ┌──────────────────┐
│ AudioWorklet │ ──────────────────────► │ streaming-core   │
│ (capture)    │    (ring buffer)        │ (Rust worker)    │
└──────────────┘                         └──────────────────┘
       │                                          │
       │ No copies!                               │
       │ Worklet writes directly                  ▼
       │ to shared memory              ┌──────────────────┐
       │                               │ AudioStreamHandle│
       │                               │ (index into pool)│
       └───────────────────────────────┴──────────────────┘
```

**AudioWorklet (browser):**
```javascript
// processor.js - runs on audio thread
class CaptureProcessor extends AudioWorkletProcessor {
  constructor() {
    // Get SharedArrayBuffer from main thread
    this.ringBuffer = new Float32Array(sharedArrayBuffer);
    this.writePtr = 0;
  }

  process(inputs) {
    const input = inputs[0][0]; // mono
    // Write directly to shared memory - NO COPY
    this.ringBuffer.set(input, this.writePtr);
    this.writePtr = (this.writePtr + input.length) % this.ringBuffer.length;
    return true;
  }
}
```

**Rust worker:**
```rust
// streaming-core/src/audio/capture.rs
pub struct AudioCaptureStream {
    shared_buffer: &'static [f32],  // Maps to SharedArrayBuffer
    read_ptr: AtomicUsize,
    handle: AudioStreamHandle,
}

impl AudioCaptureStream {
    pub fn read_samples(&self, out: &mut [f32]) -> usize {
        // Read from shared buffer - NO COPY from JS
        // Audio data never touched JavaScript heap
    }
}
```

### Rust → Browser (Playback)

```
┌──────────────────┐    SharedArrayBuffer    ┌──────────────┐
│ streaming-core   │ ──────────────────────► │ AudioWorklet │
│ (mixer output)   │    (ring buffer)        │ (playback)   │
└──────────────────┘                         └──────────────┘
```

Same pattern reversed - Rust writes, Worklet reads.

---

## Mixing Architecture

**Per-call mixer (Rust):**
```rust
pub struct CallMixer {
    call: CallHandle,
    inputs: Vec<AudioStreamHandle>,     // All participant streams
    outputs: HashMap<ParticipantHandle, AudioMixHandle>,  // Mix-minus per participant
}

impl CallMixer {
    pub fn mix_frame(&mut self) {
        // For each participant, mix all OTHER participants
        for (participant, output) in &mut self.outputs {
            let mut mix = [0.0f32; FRAME_SIZE];
            for input in &self.inputs {
                if input.owner != *participant {
                    // Accumulate into mix buffer
                    self.accumulate(*input, &mut mix);
                }
            }
            // Write to output handle's buffer
            output.write(&mix);
        }
    }
}
```

**Mix-minus:** Each participant hears everyone EXCEPT themselves. This prevents echo and is standard for all conferencing systems.

---

## AI Voice Injection

AI participants don't have microphones. They inject audio via TTS:

```rust
pub struct AIParticipant {
    participant: ParticipantHandle,
    tts_stream: AudioStreamHandle,  // TTS output treated like any other stream
    voice_config: VoiceConfig,
}

impl AIParticipant {
    pub fn speak(&mut self, text: &str) {
        // TTS generates audio directly into the stream handle's buffer
        // NO copying through JavaScript
        self.tts_engine.synthesize_into(text, &mut self.tts_stream);
    }
}
```

The mixer sees AI audio streams identically to human mic streams.

---

## Command Buffer Pattern

**Problem:** Synchronous RPC for each action = latency, no batching

**Solution:** Buffer commands, submit atomically, execute on streaming thread

```rust
pub enum CallCommand {
    Join { call: CallHandle, user_id: UUID },
    Leave { call: CallHandle, user_id: UUID },
    SetAudioInput { participant: ParticipantHandle, stream: AudioStreamHandle },
    SetAudioOutput { participant: ParticipantHandle, mix: AudioMixHandle },
    Mute { participant: ParticipantHandle },
    Unmute { participant: ParticipantHandle },
    InjectTTS { participant: ParticipantHandle, text: String },
    // Video commands...
}

pub struct CallCommandBuffer {
    commands: Vec<CallCommand>,
}

impl CallCommandBuffer {
    pub fn submit(self) -> CommandBufferHandle {
        // Send to streaming thread
        // Returns handle to check completion
    }
}
```

**TypeScript side:**
```typescript
// Batch multiple operations
const buffer = CallCommandBuffer.create();
buffer.push({ type: 'join', callId, userId });
buffer.push({ type: 'setAudioInput', participantId, streamHandle });
buffer.push({ type: 'unmute', participantId });
const receipt = await buffer.submit();
```

---

## Video Architecture (Future)

Same principles:
- VideoFrameHandle points to GPU texture or shared memory
- Encoder/decoder in Rust (via ffmpeg or WebCodecs bridge)
- Never copy pixels through JS
- SFU (Selective Forwarding Unit) pattern for multi-party

```
┌────────────┐   ┌────────────┐   ┌────────────┐
│ Camera     │   │ Screen     │   │ Game       │
│ Capture    │   │ Share      │   │ Stream     │
└─────┬──────┘   └─────┬──────┘   └─────┬──────┘
      │                │                │
      ▼                ▼                ▼
   VideoFrameHandle (all point to native buffers)
      │                │                │
      └────────────────┼────────────────┘
                       ▼
              ┌────────────────┐
              │ Video Router   │
              │ (SFU pattern)  │
              └────────────────┘
                       │
         ┌─────────────┼─────────────┐
         ▼             ▼             ▼
    Participant A  Participant B  Participant C
    (receives B,C) (receives A,C) (receives A,B)
```

---

## Commands (AI-Executable)

### Call Lifecycle
```
collaboration/call/start     --participants="helper,teacher"
collaboration/call/join      --callId="..."
collaboration/call/leave     --callId="..."
collaboration/call/end       --callId="..."  (owner only)
collaboration/call/list      --roomId="..." | --active=true
```

### Participant Management
```
collaboration/call/invite    --callId="..." --userId="..."
collaboration/call/kick      --callId="..." --userId="..."
collaboration/call/participants --callId="..."
```

### Audio Control
```
collaboration/call/mute      --callId="..."
collaboration/call/unmute    --callId="..."
collaboration/call/speak     --callId="..." --text="Hello"  (AI TTS)
collaboration/call/volume    --callId="..." --userId="..." --level=0.8
```

### Video Control
```
collaboration/call/camera    --callId="..." --enabled=true
collaboration/call/screen-share --callId="..." --enabled=true
collaboration/call/video-layout --callId="..." --layout="grid|spotlight|strip"
```

### Status/Signaling
```
collaboration/call/raise-hand  --callId="..."
collaboration/call/react       --callId="..." --emoji="👍"
collaboration/call/status      --callId="..."  (who's muted, sharing, etc.)
```

---

## Module Structure

```
workers/streaming-core/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── handles.rs           # Handle types and pools
│   ├── command_buffer.rs    # Command buffer impl
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── capture.rs       # Mic input (SharedArrayBuffer)
│   │   ├── playback.rs      # Speaker output
│   │   ├── mixer.rs         # Multi-party mixing
│   │   ├── stream.rs        # AudioStreamHandle impl
│   │   └── tts.rs           # AI voice synthesis
│   ├── video/
│   │   ├── mod.rs
│   │   ├── capture.rs       # Camera/screen capture
│   │   ├── encoder.rs       # H.264/VP9 encoding
│   │   ├── decoder.rs       # Decoding
│   │   └── router.rs        # SFU routing
│   └── transport/
│       ├── mod.rs
│       ├── webrtc.rs        # WebRTC data channels
│       └── websocket.rs     # Fallback transport

commands/collaboration/call/
├── start/                   # Create call + join
├── join/                    # Join existing
├── leave/                   # Leave call
├── end/                     # End call (owner)
├── invite/                  # Add participant
├── kick/                    # Remove participant
├── mute/                    # Mute self
├── unmute/                  # Unmute self
├── speak/                   # AI TTS injection
├── camera/                  # Toggle video
├── screen-share/            # Toggle screen share
├── participants/            # List participants
├── status/                  # Call status
└── README.md

widgets/call/
├── CallWidget.ts            # Full video grid view
├── CallBarWidget.ts         # Minimal overlay bar
├── CallControlsWidget.ts    # Mic/cam/share buttons
└── ParticipantTileWidget.ts # Individual participant tile

system/call/
├── CallManager.ts           # State machine, handle bookkeeping
├── CallEntity.ts            # Database entity (renamed from LiveSession)
├── CallEvents.ts            # Event types
└── CallHandles.ts           # TypeScript handle wrappers
```

---

## Testing Strategy

### Unit Tests (No audio hardware)
- Handle pool allocation/deallocation
- Command buffer serialization
- Mix-minus algorithm (synthetic inputs)
- State machine transitions

### Integration Tests (Loopback)
- Capture → immediate playback (loopback)
- Rust worker communication
- Handle lifecycle across JS↔Rust boundary

### System Tests (Multiple participants)
- Two browser tabs in same call
- AI participant joins and speaks
- Network simulation (latency, packet loss)

### Performance Tests
- Audio latency (target: <50ms end-to-end)
- CPU usage with N participants
- Memory usage (no leaks over time)
- Handle pool exhaustion recovery

---

## Implementation Phases

### Phase 1: Rename + Foundation
- [x] Rename LiveSession → Call (CallEntity, CallParticipant, CallStatus)
- [x] Update EntityRegistry to use CallEntity
- [x] Update command types to use CallEntity imports
- [ ] Create streaming-core Rust worker skeleton
- [ ] Define handle types in Rust
- [ ] SharedArrayBuffer setup between browser ↔ Rust

### Phase 2: Audio Capture
- [ ] AudioWorklet capture processor
- [ ] Ring buffer shared memory
- [ ] Rust worker reads from shared buffer
- [ ] AudioStreamHandle creation

### Phase 3: Audio Playback
- [ ] Rust worker writes to output buffer
- [ ] AudioWorklet playback processor
- [ ] Loopback test (mic → speaker)

### Phase 4: Mixing
- [ ] Multi-stream mixer in Rust
- [ ] Mix-minus per participant
- [ ] Two-participant call test

### Phase 5: AI Voice
- [ ] TTS integration in Rust (or call external service)
- [ ] AI participant speaks into call
- [ ] speak command

### Phase 6: Transport
- [ ] WebSocket transport for audio packets
- [ ] Server-side relay
- [ ] Multi-device test

### Phase 7: Video (Future)
- [ ] Video capture handles
- [ ] Encoding/decoding
- [ ] SFU routing

### Phase 8: WebRTC (Optimization)
- [ ] P2P when possible
- [ ] TURN/STUN for NAT traversal
- [ ] Quality adaptation

---

## Open Questions

1. **TTS engine**: Run in Rust (Piper/Coqui) or call external API (ElevenLabs)?
2. **WebRTC vs WebSocket**: Start with WS for simplicity, add WebRTC later?
3. **Video codecs**: VP9 (royalty-free) vs H.264 (hardware support)?
4. **Game streaming**: Capture game window? OBS-style? Direct GPU access?

---

## References

- [bgfx](https://github.com/bkaradzic/bgfx) - Handle-based graphics architecture
- [WebCodecs API](https://developer.mozilla.org/en-US/docs/Web/API/WebCodecs_API) - Browser video encoding
- [AudioWorklet](https://developer.mozilla.org/en-US/docs/Web/API/AudioWorklet) - Low-latency audio processing
- [SharedArrayBuffer](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/SharedArrayBuffer) - Zero-copy browser↔worker
