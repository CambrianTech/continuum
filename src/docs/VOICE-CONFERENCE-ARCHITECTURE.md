# Voice Conference Architecture

## Vision

Multi-party voice conferencing where humans and AIs talk simultaneously - like a Zoom call where everyone can speak and hear each other in real-time.

## Current State vs Target

| Aspect | Current (1:1) | Target (N:N) |
|--------|---------------|--------------|
| Model | Request/response | Continuous streams |
| Participants | 1 human + 1 AI | N humans + M AIs |
| Audio flow | Sequential | Parallel/mixed |
| Turn-taking | Explicit (you speak, AI responds) | Natural (interrupts, overlaps) |

## Architecture Overview

```
                    ┌────────────────────────────────┐
                    │      ConferenceRoom            │
                    │  ┌──────────────────────────┐  │
                    │  │     AudioMixer           │  │
                    │  │  - Mix N→1 per output    │  │
                    │  │  - Exclude self (echo)   │  │
                    │  │  - Normalize levels      │  │
                    │  └──────────────────────────┘  │
                    │  ┌──────────────────────────┐  │
                    │  │   ParticipantRegistry    │  │
                    │  │  - Track who's in room   │  │
                    │  │  - Handle join/leave     │  │
                    │  │  - Mute states           │  │
                    │  └──────────────────────────┘  │
                    │  ┌──────────────────────────┐  │
                    │  │   SpeakerDetection       │  │
                    │  │  - VAD per participant   │  │
                    │  │  - Active speaker events │  │
                    │  └──────────────────────────┘  │
                    └───────────┬────────────────────┘
                                │
         ┌──────────┬───────────┼───────────┬──────────┐
         │          │           │           │          │
         ▼          ▼           ▼           ▼          ▼
    ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐
    │ Human   │ │ Helper  │ │ Teacher │ │ DeepSeek│ │ Human   │
    │ Joel    │ │ AI      │ │ AI      │ │ AI      │ │ Guest   │
    │ (WebRTC)│ │(virtual)│ │(virtual)│ │(virtual)│ │ (WebRTC)│
    └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘
```

## Components

### 1. ConferenceRoom (Rust)

Central hub managing a voice conference:

```rust
pub struct ConferenceRoom {
    room_id: String,
    participants: HashMap<Handle, Participant>,
    mixer: AudioMixer,
    event_bus: Arc<EventBus>,

    // Turn coordination (re-use ThoughtStreamCoordinator logic)
    active_speaker: Option<Handle>,
    speaker_queue: VecDeque<Handle>,
}

pub struct Participant {
    handle: Handle,
    participant_type: ParticipantType,  // Human, AI
    name: String,
    muted: bool,

    // Audio I/O
    input_buffer: RingBuffer<AudioFrame>,   // Audio from this participant
    output_buffer: RingBuffer<AudioFrame>,  // Mixed audio TO this participant

    // For AI participants
    stt_pipeline: Option<Pipeline>,
    tts_pipeline: Option<Pipeline>,
}

enum ParticipantType {
    Human { transport: WebRtcTransport },
    AI { persona_id: Uuid, model_id: String },
}
```

### 2. AudioMixer (Rust)

Mixes N audio streams, outputs M streams (each excluding self):

```rust
pub struct AudioMixer {
    sample_rate: u32,
    frame_size: usize,  // 20ms = 320 samples at 16kHz
}

impl AudioMixer {
    /// Mix all participant audio, returning separate mix for each
    /// (excluding their own audio to prevent echo)
    pub fn mix_frame(
        &self,
        inputs: &HashMap<Handle, AudioFrame>,
    ) -> HashMap<Handle, AudioFrame> {
        let mut outputs = HashMap::new();

        for (target_handle, _) in inputs {
            // Mix all OTHER participants for this target
            let mixed: Vec<i16> = inputs.iter()
                .filter(|(h, _)| *h != target_handle)
                .map(|(_, frame)| &frame.samples)
                .fold(vec![0i32; self.frame_size], |mut acc, samples| {
                    for (i, &s) in samples.iter().enumerate() {
                        acc[i] += s as i32;
                    }
                    acc
                })
                .into_iter()
                .map(|s| (s.clamp(-32768, 32767)) as i16)
                .collect();

            outputs.insert(*target_handle, AudioFrame::new(mixed, 0, self.sample_rate));
        }

        outputs
    }
}
```

### 3. AI Participant Pipeline

Each AI in the conference runs:

```
Incoming Mix → VAD → STT → [PersonaUser] → TTS → Outgoing Audio
                                ↓
                    Uses ThoughtStreamCoordinator
                    for turn-taking decisions
```

```rust
impl Participant {
    async fn process_incoming_audio(&mut self, mixed: AudioFrame) -> Option<AudioFrame> {
        // 1. Run VAD - is anyone speaking?
        let vad_result = self.vad_stage.process(Frame::Audio(mixed)).await?;

        // 2. Transcribe what was said
        let text = self.stt_stage.process(vad_result).await?;

        // 3. Decide if we should respond (ThoughtStreamCoordinator)
        if !self.should_respond(&text).await {
            return None;
        }

        // 4. Generate response via PersonaUser
        let response = self.generate_response(&text).await?;

        // 5. Synthesize to audio
        let audio = self.tts_stage.process(Frame::Text(response)).await?;

        Some(audio)
    }
}
```

### 4. Human Participant (WebRTC)

Browsers connect via WebRTC for low-latency:

```
Browser (WebRTC) ←→ streaming-core (Rust) ←→ ConferenceRoom
```

WebRTC gives us:
- Low latency (<100ms)
- Built-in echo cancellation (AEC)
- Jitter buffering
- Opus codec (efficient)

### 5. Room Management Commands

```bash
# Create/join conference
./jtag conference/start --room="general"
# Returns: { conferenceId, wsUrl, participants: [...] }

# Join existing conference
./jtag conference/join --conferenceId="abc123"

# Leave conference
./jtag conference/leave --conferenceId="abc123"

# Mute/unmute
./jtag conference/mute --conferenceId="abc123" --participant="helper-ai"

# List participants
./jtag conference/participants --conferenceId="abc123"
```

## Audio Flow (20ms Frame Loop)

```
Every 20ms:
┌──────────────────────────────────────────────────────────────┐
│ 1. Collect audio from all participants                       │
│    - WebRTC: read from tracks                                │
│    - AI: read from TTS output buffer (if speaking)           │
│                                                              │
│ 2. Mix audio for each participant                            │
│    - Each gets mix of everyone else                          │
│                                                              │
│ 3. Distribute mixed audio                                    │
│    - WebRTC: write to outgoing tracks                        │
│    - AI: feed to STT input buffer                            │
│                                                              │
│ 4. AI processing (async, parallel)                           │
│    - Each AI runs: VAD → STT → think → TTS                   │
│    - ThoughtStreamCoordinator prevents crosstalk             │
└──────────────────────────────────────────────────────────────┘
```

## Turn-Taking Strategy

Prevent AI chaos (everyone talking at once):

1. **VAD triggers** - Only process when someone stops speaking
2. **Response queue** - AIs queue responses, one speaks at a time
3. **Priority** - Direct address (e.g., "Hey Helper") gets priority
4. **Timeout** - If no one responds in 2s, next in queue can go
5. **Barge-in** - Humans can always interrupt (detected by VAD)

```rust
struct TurnCoordinator {
    current_speaker: Option<Handle>,
    response_queue: VecDeque<(Handle, String)>,
    last_speech_end: Instant,
}

impl TurnCoordinator {
    fn request_turn(&mut self, handle: Handle, response: String) -> bool {
        if self.current_speaker.is_none()
            && self.last_speech_end.elapsed() > Duration::from_millis(500)
        {
            self.current_speaker = Some(handle);
            true
        } else {
            self.response_queue.push_back((handle, response));
            false
        }
    }

    fn on_speech_end(&mut self, handle: Handle) {
        if self.current_speaker == Some(handle) {
            self.current_speaker = None;
            self.last_speech_end = Instant::now();

            // Let next in queue speak
            if let Some((next, _)) = self.response_queue.pop_front() {
                self.current_speaker = Some(next);
            }
        }
    }
}
```

## Implementation Phases

### Phase 1: Conference Room Foundation (Rust)
- [ ] `ConferenceRoom` struct with participant management
- [ ] `AudioMixer` for N→N-1 mixing
- [ ] gRPC service for room management
- [ ] Basic join/leave/mute

### Phase 2: AI Participants
- [ ] Wire existing PersonaUser into conference
- [ ] Adapt ThoughtStreamCoordinator for voice turn-taking
- [ ] Continuous STT on mixed audio feed
- [ ] Response queueing

### Phase 3: Human Participants (WebRTC)
- [ ] WebRTC signaling server
- [ ] ICE/STUN/TURN for NAT traversal
- [ ] Browser WebRTC integration in VoiceChatWidget
- [ ] Echo cancellation (AEC)

### Phase 4: Quality & Polish
- [ ] Noise suppression
- [ ] Automatic gain control
- [ ] Active speaker detection (visual indicator)
- [ ] Recording/transcription of full conference

### Phase 5: Video Extension
- [ ] Add video tracks to WebRTC
- [ ] AI avatar rendering (LivePortrait/SadTalker)
- [ ] Video mixing/layout

### Phase 6: Avatars
- [ ] AI-driven lip sync (audio → mouth movements)
- [ ] Expression matching (sentiment → face)
- [ ] Virtual backgrounds

## Files to Create

```
/workers/streaming-core/src/
├── conference.rs       # ConferenceRoom, Participant
├── mixer.rs            # AudioMixer
├── turn_coordinator.rs # Turn-taking logic
└── webrtc.rs           # WebRTC integration (or separate crate)

/commands/conference/
├── start/              # Create/join conference
├── join/               # Join existing
├── leave/              # Leave conference
├── mute/               # Mute participant
└── participants/       # List participants

/system/conference/
├── shared/ConferenceTypes.ts
├── server/ConferenceManager.ts
└── browser/ConferenceWidget.ts
```

## Technology Options

### WebRTC Libraries (Rust)
- **webrtc-rs**: Pure Rust, actively maintained
- **libwebrtc**: Google's C++ lib with Rust bindings

### Audio Processing
- **rubato**: High-quality resampling
- **dasp**: Audio sample processing
- **cpal**: Cross-platform audio I/O (for desktop apps)

### Signaling
- Use existing WebSocket infrastructure
- Or add dedicated signaling endpoint

## Questions to Decide

1. **SFU vs MCU?**
   - SFU: Forward streams, clients mix (less server CPU)
   - MCU: Server mixes (our AudioMixer approach - simpler for AIs)
   - **Recommendation**: MCU for AI participants, optional SFU for human-to-human

2. **Existing rooms or new conference entity?**
   - Extend existing room model with voice mode?
   - Or separate Conference entity that references a room?

3. **How many simultaneous AIs?**
   - 4-5 works conversationally
   - 10+ gets chaotic even with coordination
   - **Recommendation**: Default cap of 5 AI participants

4. **Recording?**
   - Store mixed audio? Individual tracks?
   - Transcription for searchable archive?

## Verification

- [ ] 2+ humans can talk simultaneously
- [ ] AI can hear mixed audio and respond
- [ ] Turn-taking prevents AI pile-on
- [ ] Latency <200ms end-to-end
- [ ] Echo cancellation works (no feedback loops)
- [ ] Join/leave works cleanly
