# Real-Time Voice Architecture

## Status

**Validated**: Browser → WebSocket → Rust → WebSocket → Browser audio pipeline works.
Current implementation has echo/latency issues due to hacky design, but proves the path is viable.

**Next**: Proper architecture with efficiency, concurrency, and natural conversation latency.

---

## The Room's Needs

A room with 3 humans + 5 AIs needs:

```
┌─────────────────────────────────────────────────────────────────┐
│                          ROOM                                    │
│                                                                  │
│  Humans: Joel, Alice, Bob (mics, cameras)                       │
│  AIs: Helper, Teacher, CodeReview, Claude, GPT (voices, avatars)│
│                                                                  │
│  Requirements:                                                   │
│  • Everyone hears everyone else (mix-minus)                      │
│  • One shared transcript (not N separate STT streams)           │
│  • AI turn-taking (not all 5 responding at once)                │
│  • Each AI has distinct voice                                    │
│  • <500ms latency for natural conversation                      │
│  • Video grid with avatars for AIs                              │
└─────────────────────────────────────────────────────────────────┘
```

---

## Concurrency Model

```
┌─────────────────────────────────────────────────────────────────┐
│                    RUST STREAMING-CORE                          │
│                   (Real-time, zero-copy)                        │
│                                                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐             │
│  │ Audio Thread│  │ STT Thread  │  │ TTS Thread  │             │
│  │             │  │             │  │             │             │
│  │ Mix-minus   │  │ Whisper     │  │ Kokoro      │             │
│  │ per client  │  │ streaming   │  │ streaming   │             │
│  │             │  │             │  │             │             │
│  │ 10ms frames │  │ Utterance   │  │ Chunk-based │             │
│  │ lock-free   │  │ detection   │  │ output      │             │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘             │
│         │                │                │                     │
│         └────────────────┼────────────────┘                     │
│                          │                                      │
│                    Ring Buffers                                 │
│                    (lock-free SPSC)                             │
└─────────────────────────────────────────────────────────────────┘
                           │
                           │ Events (utterance complete, etc.)
                           ▼
┌─────────────────────────────────────────────────────────────────┐
│                    TYPESCRIPT ORCHESTRATOR                       │
│                   (Conversation logic only)                      │
│                                                                  │
│  • Receives: "Joel said: How do I fix this bug?"                │
│  • Decides: Which AI(s) should respond?                         │
│  • Coordinates: Teacher first, then CodeReview if needed        │
│  • Sends: "Teacher, respond to this"                            │
│  • Receives: "Teacher says: Let me explain..."                  │
│  • Sends to TTS: "Speak this with Teacher's voice"              │
│                                                                  │
│  *** NO AUDIO BYTES EVER TOUCH THIS LAYER ***                   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Key Design Decisions

### 1. Single STT Stream Per Room

```rust
// WRONG: N participants = N STT instances = N × CPU
for participant in room.participants {
    whisper.transcribe(participant.audio); // 💀
}

// RIGHT: Mixed audio → single STT → shared transcript
let mixed = mixer.get_room_audio();
let transcript = whisper.transcribe(mixed);
room.broadcast_transcript(transcript);
```

### 2. AI Turn-Taking Arbitration

```typescript
interface TurnArbiter {
  // When human finishes speaking, who responds?
  selectResponder(
    utterance: Utterance,
    availableAIs: AI[],
    conversationContext: Context
  ): AI | null;

  // Can this AI interrupt?
  canInterrupt(ai: AI, currentSpeaker: Participant): boolean;
}

// Strategies:
// - RelevanceArbiter: Score by topic relevance
// - RoundRobinArbiter: Take turns
// - DirectedArbiter: User explicitly addresses an AI
// - SilenceArbiter: No one responds unless confidence high
```

### 3. Lock-Free Audio Pipeline

```rust
struct ParticipantOutput {
    buffer: RingBuffer<f32>,  // Lock-free SPSC
    write_pos: AtomicUsize,
    read_pos: AtomicUsize,
}

// Audio thread never blocks
fn audio_tick(&mut self) {
    for participant in &mut self.participants {
        if let Some(samples) = participant.input.try_read() {
            self.mixer.add(participant.id, samples);
        }
        let output = self.mixer.get_mix_minus(participant.id);
        participant.output.try_write(output);
    }
}
```

### 4. Latency Budget (500ms total)

```
├── Network in:        50ms  (WebSocket)
├── Audio buffering:   60ms  (3 × 20ms frames)
├── STT:              150ms  (streaming Whisper)
├── AI decision:       50ms  (which AI responds)
├── AI inference:       0ms  (async, pre-compute while STT)
├── TTS first chunk:  100ms  (streaming Kokoro)
├── Network out:       50ms  (WebSocket)
└── Audio playout:     40ms  (2 × 20ms frames)
                      ─────
                      500ms
```

### 5. Speculative Execution

```typescript
// While human is still speaking, AIs start thinking
onPartialTranscript(partial: string) {
  // "How do I fix this b..."
  this.speculativeResponses = this.ais.map(ai =>
    ai.generateSpeculative(partial)
  );
}

onUtteranceComplete(final: string) {
  // Use speculative if valid, else fresh (but had head start)
}
```

---

## Data Flow

```
Human speaks
    │
    ▼
┌─────────────────────────────────────────┐
│           WebSocket (binary)             │
│         Opus-encoded audio               │
└─────────────────┬───────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────┐
│         Rust Audio Thread               │
│  • Decode Opus                          │
│  • Add to mixer                         │
│  • Generate mix-minus for others        │
│  • Feed to STT thread                   │
└─────────────────┬───────────────────────┘
                  │
    ┌─────────────┼─────────────┐
    │             │             │
    ▼             ▼             ▼
Mix-minus     STT Thread    Speculative
to others     (Whisper)     AI inference
    │             │             │
    │             ▼             │
    │      Utterance complete   │
    │             │             │
    │             ▼             │
    │      ┌──────────────┐    │
    │      │ Orchestrator │◄───┘
    │      │  (TypeScript)│
    │      └──────┬───────┘
    │             │
    │             ▼
    │      Select AI responder
    │             │
    │             ▼
    │      AI generates text
    │             │
    │             ▼
    │      TTS Thread (Kokoro)
    │             │
    │             ▼
    │      Audio chunks
    │             │
    └─────────────┼─────────────┐
                  │             │
                  ▼             ▼
            To speaker      To others
            (AI voice)     (mix-minus)
```

---

## Interfaces

### TypeScript → Rust

```typescript
interface AudioEngine {
  joinRoom(roomId: RoomId, userId: UserId): ParticipantHandle;
  leaveRoom(handle: ParticipantHandle): void;
  setMuted(handle: ParticipantHandle, muted: boolean): void;
  speak(text: string, voiceId: VoiceId): SpeechHandle;
  cancelSpeech(handle: SpeechHandle): void;
}
```

### Rust → TypeScript (Events)

```typescript
interface ConversationOrchestrator {
  onUtterance(event: {
    speakerId: UserId;
    transcript: string;
    audioRef: AudioHandle;
    confidence: number;
  }): void;

  onPartialTranscript(event: {
    speakerId: UserId;
    partial: string;
  }): void;

  onSpeakingStateChange(event: {
    participantId: UserId;
    isSpeaking: boolean;
  }): void;
}
```

---

## File Structure (Target)

```
workers/streaming-core/
├── src/
│   ├── audio/
│   │   ├── mixer.rs          # Mix-minus logic
│   │   ├── ring_buffer.rs    # Lock-free buffers
│   │   └── codec.rs          # Opus encode/decode
│   ├── stt/
│   │   ├── whisper.rs        # Whisper integration
│   │   ├── vad.rs            # Voice activity detection
│   │   └── utterance.rs      # Utterance boundary detection
│   ├── tts/
│   │   ├── kokoro.rs         # Kokoro TTS
│   │   └── voice_bank.rs     # Voice profiles
│   ├── room/
│   │   ├── state.rs          # Room state machine
│   │   ├── participant.rs    # Per-participant state
│   │   └── events.rs         # Events to TypeScript
│   └── main.rs

system/conversation/
├── Orchestrator.ts           # Turn-taking, AI selection
├── TurnArbiter.ts            # Who speaks when
├── SpeculativeEngine.ts      # Pre-compute responses
└── VoiceProfile.ts           # AI voice configurations

commands/voice/
├── room-join/                # Join voice in room
├── room-leave/               # Leave voice
├── mute/                     # Mute/unmute
└── speak/                    # AI speaks (text → TTS)
```

---

## Migration Path

### Phase 1: Fix Rust Streaming-Core (Current)
- [ ] Implement proper mix-minus (don't echo back to self)
- [ ] Add VAD (voice activity detection)
- [ ] Add utterance boundary detection
- [ ] Emit events to TypeScript on utterance complete

### Phase 2: Wire STT
- [ ] Whisper integration in Rust
- [ ] Streaming transcription
- [ ] Emit partial + final transcripts

### Phase 3: Orchestration Layer
- [ ] Create ConversationOrchestrator in TypeScript
- [ ] Implement TurnArbiter (simple relevance-based)
- [ ] Post transcripts to chat room
- [ ] Trigger AI responses

### Phase 4: Wire TTS
- [ ] Kokoro integration in Rust
- [ ] Voice profiles per persona
- [ ] Streaming audio output
- [ ] Mix AI audio into room

### Phase 5: Polish
- [ ] Speculative execution
- [ ] Interruption handling
- [ ] Latency optimization
- [ ] Video/avatar integration

---

## Persona Inbox Integration

**Key Insight**: Voice is a **modality**, not a domain. An AI in a voice call is doing `chat` domain work, just with audio I/O instead of text. This means:

- Same `InboxMessage` structure works for both text and voice
- Each voice call is a different `roomId` (multi-context support)
- AI can be in text chat + multiple voice calls simultaneously
- All contexts flow through the same priority queue

### Multi-Context Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                      PersonaInbox (Helper AI)                    │
│                      (Priority Queue)                            │
│                                                                  │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ [0.8] Voice: Joel asked "How do I fix this?" (call-abc)  │   │
│  │ [0.7] Text: @Helper can you review? (general room)       │   │
│  │ [0.5] Voice: Alice said "Any updates?" (call-xyz)        │   │
│  │ [0.3] Text: Random discussion (general room)             │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                  │
│  CNS processes highest priority first                            │
│  Response routes to appropriate output (TTS vs text)             │
└─────────────────────────────────────────────────────────────────┘
```

### InboxMessage Extension

```typescript
// QueueItemTypes.ts - add sourceModality
export interface InboxMessage extends BaseQueueItem {
  type: 'message';
  roomId: UUID;              // Room or voice session ID
  content: string;           // Text or transcribed audio
  senderId: UUID;
  senderName: string;
  senderType: 'human' | 'persona' | 'agent' | 'system';
  mentions?: boolean;

  // NEW: Modality tracking for response routing
  sourceModality?: 'text' | 'voice';   // Where input came from
  voiceSessionId?: UUID;               // Voice call context if applicable
}
```

### VoiceOrchestrator Integration

```typescript
/**
 * VoiceOrchestrator bridges Rust streaming-core ↔ PersonaInbox
 *
 * Responsibilities:
 * 1. Receive transcription events from Rust
 * 2. Create InboxMessages for participating personas
 * 3. Turn arbitration (which AI responds)
 * 4. Route responses to TTS
 */
class VoiceOrchestrator {
  // Called when Rust emits utterance complete
  async onUtterance(event: UtteranceEvent): Promise<void> {
    const { sessionId, speakerId, transcript, speakerName } = event;

    // Get participating personas for this voice session
    const participants = await this.getVoiceParticipants(sessionId);
    const personas = participants.filter(p => p.type === 'persona');

    // Turn arbitration: which AI should respond?
    const responder = await this.arbiter.selectResponder(
      event,
      personas,
      this.getConversationContext(sessionId)
    );

    if (!responder) return; // No AI should respond

    // Create InboxMessage for selected responder
    const message: InboxMessage = {
      id: generateUUID(),
      type: 'message',
      domain: 'chat',
      roomId: sessionId,           // Voice session = room
      content: transcript,
      senderId: speakerId,
      senderName: speakerName,
      senderType: 'human',
      priority: this.calculateVoicePriority(event, responder),
      timestamp: Date.now(),
      sourceModality: 'voice',     // Route response to TTS
      voiceSessionId: sessionId
    };

    // Enqueue to responder's inbox
    await responder.inbox.enqueue(message);
  }

  // Called when persona generates response
  async onPersonaResponse(
    personaId: UUID,
    response: string,
    originalMessage: InboxMessage
  ): Promise<void> {
    if (originalMessage.sourceModality === 'voice') {
      // Route to TTS
      await this.speakResponse(
        originalMessage.voiceSessionId!,
        personaId,
        response
      );
    } else {
      // Normal text posting (existing flow)
    }
  }
}
```

### Arbitration Strategies

```typescript
interface TurnArbiter {
  selectResponder(
    event: UtteranceEvent,
    candidates: PersonaUser[],
    context: ConversationContext
  ): PersonaUser | null;
}

// Strategy 1: Directed addressing
class DirectedArbiter implements TurnArbiter {
  selectResponder(event, candidates, context): PersonaUser | null {
    // "Hey Teacher, explain this" → Teacher responds
    for (const persona of candidates) {
      const nameLower = persona.displayName.toLowerCase();
      if (event.transcript.toLowerCase().includes(nameLower)) {
        return persona;
      }
    }
    return null;
  }
}

// Strategy 2: Topic relevance
class RelevanceArbiter implements TurnArbiter {
  selectResponder(event, candidates, context): PersonaUser | null {
    // Score each AI by topic expertise
    const scores = candidates.map(p => ({
      persona: p,
      score: this.calculateRelevance(event.transcript, p.expertise)
    }));

    const best = scores.sort((a, b) => b.score - a.score)[0];
    return best.score > 0.5 ? best.persona : null;
  }
}

// Strategy 3: Composite (directed first, then relevance)
class CompositeArbiter implements TurnArbiter {
  private directed = new DirectedArbiter();
  private relevance = new RelevanceArbiter();

  selectResponder(event, candidates, context): PersonaUser | null {
    // Direct address takes precedence
    const directed = this.directed.selectResponder(event, candidates, context);
    if (directed) return directed;

    // Fall back to topic relevance
    return this.relevance.selectResponder(event, candidates, context);
  }
}
```

### Response Routing

```typescript
// In PersonaResponseGenerator.ts - after generating response
async postResponse(
  response: string,
  originalMessage: InboxMessage,
  roomId: UUID
): Promise<void> {
  if (originalMessage.sourceModality === 'voice') {
    // Voice response: Send to TTS, then inject audio into call
    await VoiceOrchestrator.instance.speakResponse(
      originalMessage.voiceSessionId!,
      this.personaId,
      response
    );

    // Also post as text for transcript
    await this.postToChat(roomId, response, { isVoiceTranscript: true });
  } else {
    // Normal text response
    await this.postToChat(roomId, response);
  }
}
```

---

## What We Validated

Browser-side proof of concept confirmed:
- ✅ getUserMedia captures mic audio
- ✅ WebSocket streams audio to Rust
- ✅ Rust receives and processes audio
- ✅ Rust sends audio back via WebSocket
- ✅ Browser plays received audio

Issues with current implementation (expected, design was hacky):
- ❌ Echo (no mix-minus)
- ❌ High latency (~1 second)
- ❌ Choppy audio (buffer issues)
- ❌ No STT/TTS integration

These are all fixable with proper architecture above.
