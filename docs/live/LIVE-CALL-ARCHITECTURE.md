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

## A Mind in the Call: Everyone Perceived Together, a Fast Voice Augmented by Deep Thought

Status: proposed, 2026-11-20. Joel: "she's seeing and hearing all of them together ... fast low
latency speaking and seeing while not losing deeper thinking models. Just augmented."

What residents do together (Joel, 2026-11-20): they join live calls together; they watch videos
together and describe them; they see and describe each other's appearance; they listen to things,
and to each other. Humans are optional: N residents can meet with no one else present, in a call or
living in a shared embodied world (a Sims-like environment) with complete embodied autonomy. How
many share one node is bounded by optimization today, never by a fixed count (plan goal 9). The
staged receipts are in [TWO-PERSONA-EMBODIED-CALL-PROOF.md](../planning/TWO-PERSONA-EMBODIED-CALL-PROOF.md)
(stage 3b: shared watching, describing and listening, with or without a human).

**Hearing and voice are layers around her model, like the cheap layers of sight.** For a model
with native audio in and out, speech-to-text and text-to-speech play the role YOLO plays for
vision: fast layers that emit events (a word heard, a turn starting) and speak her quick voice
while her model's own audio understanding and output carry the depth. For a model without native
audio they are a true need, the bridge that gives her hearing and speech at all (CLAUDE.md,
"Sensory Architecture"). Either way her voice is her own: unique per resident, by design, likely a
LoRA voice gene in her genome (the proof plan's "learned local voice" row).

A resident in a call is a citizen in the room, not a voice behind `speak(text)`. She hears and sees
every participant at once: the humans, and the other residents' avatars. She answers at
conversational speed, and her full mind keeps thinking underneath. The fast layers augment her
deep mind and never replace it.

**Everyone perceived together, computed once.** Each stream's perception is a property of the
content, shared by every mind in the call; only attention and relevance are each mind's own
([CONCURRENT-MIND-AND-GOVERNOR.md](../architecture/CONCURRENT-MIND-AND-GOVERNOR.md) §3.3):

- per audio stream: voice activity and transcription, once, whoever listens;
- per video stream: cheap signals on every frame, once: frame differences, camera motion, and
  detectors (YOLO-class, semantic segmentation). These emit events that tell the deeper vision
  stages, her own model's multimodal vision layers among them, that they are needed and with what
  priority. A large region newly uncovered while the camera is
  still, a new slide, or a new face is an event, and the stage that cares takes priority there;
  other minds asking for the same frame share the work.

**She sees with all of it, dynamically fused.** The cheap signals, the deep networks, her own
model's vision layers and the fusion that ties them together are all her seeing; none excludes the
others, and each also tells the others when they are needed. Her sight is a living estimate of the
scene that is never finished and never blank. It starts rough and refines continuously as better knowledge becomes
available. Every source (cheap tracking every frame, a deep network's result seconds later, her own
model's vision layers, a closer or clearer view) updates it where it applies, weighted by its
confidence and its age. Better knowledge replaces weaker estimates region by region, and knowledge
that has gone stale loses weight until something fresher arrives. Nothing waits for a final answer: at every moment she sees the best estimate there is. This comes
from CBAR, Joel's real-time perception engine:

- **Slow results are brought into real time.** A deep analysis may take seconds. Its result is
  attached to the frame it came from, then carried forward by continuous feature tracking and
  fusion, so it lands on the present frame rather than on a stale one. Its observation time and
  confidence stay exposed, so 30 fps presentation never pretends to be fresh perception.
- **A rough estimate beats a gap.** CBAR filled the far end of a hallway at once with a CNN's
  estimate (depth, normals, semantics) where SLAM alone would leave it missing. The estimate is
  wrong in detail, but from the viewer's position that detail is a few pixels, and a rough answer
  is far better than a hole. Precise methods refine it as the camera moves toward it, seamlessly.
  Assumptions carry a scene until evidence breaks them: a floor or a wall extends until it is seen
  not to.
- **For a mind, the same rule:** she always holds a whole, rough understanding of the call (who is
  there, what is on screen, what is happening) and refines it where her attention goes. She never
  has a blank where a participant or a region should be.

**Hearing and speaking are fused the same way.** Comprehension and speech are not separate
pipelines with a model call in between. They are the same kind of living estimate as her sight,
with fast and deep layers fused:

- **Comprehension, fast first.** As a participant speaks, fast layers (transcription, a small model)
  build her running understanding word by word. Deeper layers refine it as they finish: meaning,
  intent, what it implies for the work.
- **Speech, fast first, deep merged in.** Her quick voice (a small, fast model speaking as her, from
  her context) can open at once: an acknowledgement, a first clause, a backchannel. Her deep mind on
  the served base works under the same turn identity, and its thought merges into the speech still
  to come. By the time she finishes talking she has given the considered answer, if the moment
  needed one. The output owner tracks what has been spoken, revises the continuation not yet
  spoken, and speaks a correction if deeper thought changes a claim already delivered. A new turn
  supersedes speech not yet spoken. Her avatar's visemes and gestures follow the audio actually
  going out, so her face moves from the first syllable.
- **The "if" is a learned, amygdala-like decision.** How much depth a moment needs (none, a little,
  a full deliberation; a glance, or a detailed look at a frame) is decided fast, from the fused
  state, by a small learned model, the same way a cheap detector tells deep vision it is needed. It
  is a routing and priority decision, never a gate on whether she may think: depth stays available
  and can be pulled in at any point of the turn. The decision learns from our own receipts (a
  correction she had to speak, a deep answer nobody needed), never from a fixed threshold.
- **One representation for hearing and speaking.** A word she hears and a word she is about to say
  are events on the same stream, so what she comprehends and what she plans to say inform each
  other as they form. Human cortex works this way: in Wernicke's area, the left posterior superior
  temporal gyrus is active both while listening and before speaking, encoding whole upcoming words
  before the motor stages and from the earliest point of production (Silva et al., Science, with
  Flinker's Perspective).

The result appears quick and smart at once: she sees and hears in detail and speaks in depth, as
people do, because every layer fuses, emits and subscribes to events, as in CBAR or any good game
engine, rather than waiting on the one before it.

**All of it governed and given its time.** Per-stream perception, speech out and avatar frames are
deadline consumers; the quick voice and her deep turn are latency consumers; learning and transfers
use the slack ([SHARED-RESOURCES.md, "Time"](../architecture/SHARED-RESOURCES.md#time-every-consumer-is-given-its-time)).
Under pressure, levels of detail step down (an unwatched avatar, a lower analysis rate) before any
deadline is missed, and nothing about the call stops her deep mind or her learning outright.
Codex's worked example in SUBSTRATE.md (#6170) traces one call under memory pressure.

**Revisions within a turn are typed, and only what was played counts as said** (Codex's review,
2026-11-20). One turn spans many revisions: a partial transcript can change from "send" to "do not
send" while a deep result or synthesized audio for the old revision is still in flight. So every
contribution carries its source speaker, its origin (observed, planned or played), its utterance and
its revision, through the existing stream and completion owner:

- a media playout receipt is the audible boundary: what was played is said, what is only queued is
  not;
- superseded queued audio and its matching visemes are dropped or replaced together;
- external actions are derived only from the current authorized intent, never from a superseded
  revision;
- a predicted or planned word is never counted as evidence that a participant said it.

Replayed as tests: a negation within the same turn, a second speaker interrupting with audio
buffered, and a late deep completion; each checks actual playout and side effects, plus
first-audio latency.

Measured on the same stimulus and context: first audible response; avatar frame interval; age of
the last genuine observation; regions or participants with no estimate at all (target zero, as
CBAR had no gaps); duplicate perception computed across residents (target zero); the
deep reply's quality with the quick voice against the deep reply alone (the augmentation must not
degrade it); and decode and learning throughput with the call against without.

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
