# Voice Synthesis Architecture

PersonaUsers can now speak in live voice calls! This document describes the architecture and how to improve TTS quality.

## Architecture Overview

```
User speaks → Rust call_server (Whisper STT) → Transcription
              ↓
VoiceOrchestrator → Posts to chat → PersonaUser sees message
              ↓
PersonaUser generates response → VoiceOrchestrator routes to TTS
              ↓
AIAudioBridge.speak() → VoiceService → voice/synthesize → gRPC
              ↓
Rust streaming-core → Piper TTS → Audio → Call server → Browser
```

## Components

### 1. VoiceOrchestrator (`system/voice/server/VoiceOrchestrator.ts`)

**Responsibilities:**
- Receives transcriptions from voice calls
- Posts transcripts to chat (all AIs see them)
- Performs turn arbitration (which AI responds via VOICE)
- Routes persona responses to TTS

**Turn Arbitration Strategies:**
1. **Direct Address**: Responds when explicitly named ("Hey Teacher...")
2. **Topic Relevance**: Scores by expertise keywords
3. **Round-Robin**: Takes turns for questions
4. **Silence for Statements**: Prevents spam

### 2. AIAudioBridge (`system/voice/server/AIAudioBridge.ts`)

**Responsibilities:**
- Connects AI participants to Rust call_server via WebSocket
- Injects TTS audio into live calls
- Handles reconnection with exponential backoff

**Key method:**
```typescript
async speak(callId: string, userId: UUID, text: string): Promise<void> {
  // 1. Use VoiceService to get TTS audio
  const voiceService = getVoiceService();
  const result = await voiceService.synthesizeSpeech({ text, userId, adapter: 'piper' });

  // 2. Stream audio to call in 20ms frames
  const frameSize = 320;  // 20ms at 16kHz
  for (let i = 0; i < result.audioSamples.length; i += frameSize) {
    const frame = result.audioSamples.slice(i, i + frameSize);
    connection.ws.send(JSON.stringify({ type: 'Audio', data: base64(frame) }));
    await sleep(20);  // Real-time pacing
  }
}
```

### 3. VoiceService (`system/voice/server/VoiceService.ts`)

**Responsibilities:**
- High-level TTS API (like LLM inference pattern)
- Adapter selection (piper/kokoro/elevenlabs/etc)
- Fallback on failure
- Audio format conversion to i16

**Usage:**
```typescript
const voice = getVoiceService();
const result = await voice.synthesizeSpeech({
  text: "Hello, I'm Helper AI",
  userId: personaId,
  adapter: 'piper',  // Optional: override default
});
// result.audioSamples is i16 array ready for WebSocket
```

### 4. VoiceConfig (`system/voice/shared/VoiceConfig.ts`)

**Centralized configuration for TTS adapters:**
```typescript
export const DEFAULT_VOICE_CONFIG: VoiceConfig = {
  tts: {
    defaultAdapter: 'piper',      // Current default
    fallbackAdapter: 'macos-say', // Fallback if default fails
    adapters: {
      piper: { voice: 'af', speed: 1.0 },
      // Add more adapters here
    },
  },
  maxSynthesisTimeMs: 5000,
};
```

### 5. Rust TTS (`workers/streaming-core/src/tts/`)

**Local TTS adapters:**
- **Piper** (`piper.rs`): ONNX-based TTS, fast, basic quality (CURRENT)
- **Kokoro** (`kokoro.rs`): Better local TTS, 80.9% TTS Arena win rate (TO ADD)

**Architecture:**
- Runs off-main-thread in Rust worker
- Accessed via gRPC from TypeScript
- Returns i16 PCM audio at 16kHz

### 6. Audio Mixer (`workers/streaming-core/src/mixer.rs`)

**Multi-participant audio mixing:**
- Mix-minus: Each participant hears everyone except themselves
- AI participants: `ParticipantStream::new_ai()` - no VAD needed
- Handles muting, volume normalization

## Performance

**Current Performance (Piper TTS):**
```
Text: 178 chars → Audio: 3.44s
Synthesis time: 430ms
Realtime factor: 0.13x (fast enough for real-time!)
```

**Realtime factor:**
- `< 1.0x`: Fast enough for live calls ✅
- `1.0-2.0x`: Borderline
- `> 2.0x`: Too slow

## Improving TTS Quality

Current Piper TTS is "not much better than say command." Here's how to upgrade:

### Option 1: Kokoro (Free, Local, Better Quality)

**Quality**: 80.9% TTS Arena win rate (vs Piper ~40%)

**Steps:**
1. Download Kokoro model:
   ```bash
   cd workers/streaming-core
   python3 scripts/download_kokoro_model.py
   ```

2. Update default adapter:
   ```typescript
   // system/voice/shared/VoiceConfig.ts
   export const DEFAULT_VOICE_CONFIG: VoiceConfig = {
     tts: {
       defaultAdapter: 'kokoro',  // Changed from 'piper'
       fallbackAdapter: 'piper',  // Piper as fallback
       adapters: {
         kokoro: { voice: 'af', speed: 1.0 },
         piper: { voice: 'af', speed: 1.0 },
       },
     },
   };
   ```

3. Rebuild and deploy:
   ```bash
   npm run build:ts
   npm start
   ```

### Option 2: ElevenLabs (Paid, API, Premium Quality)

**Quality**: 80%+ TTS Arena win rate, extremely natural

**Steps:**
1. Get API key from https://elevenlabs.io

2. Add to config:
   ```typescript
   // system/voice/shared/VoiceConfig.ts
   export const DEFAULT_VOICE_CONFIG: VoiceConfig = {
     tts: {
       defaultAdapter: 'elevenlabs',
       fallbackAdapter: 'piper',
       adapters: {
         elevenlabs: {
           apiKey: process.env.ELEVENLABS_API_KEY,
           voiceId: 'EXAVITQu4vr4xnSDxMaL',  // Bella
           model: 'eleven_turbo_v2',
         },
         piper: { voice: 'af', speed: 1.0 },
       },
     },
   };
   ```

3. Implement ElevenLabs adapter in Rust:
   ```rust
   // workers/streaming-core/src/tts/elevenlabs.rs
   use async_trait::async_trait;
   use crate::tts::{TTSAdapter, TTSRequest, TTSResult};

   pub struct ElevenLabsAdapter {
       api_key: String,
       voice_id: String,
       model: String,
   }

   #[async_trait]
   impl TTSAdapter for ElevenLabsAdapter {
       async fn synthesize(&self, request: &TTSRequest) -> Result<TTSResult, TTSError> {
           // HTTP request to ElevenLabs API
           // Return i16 samples at 16kHz
       }
   }
   ```

4. Register in TTS registry:
   ```rust
   // workers/streaming-core/src/tts/mod.rs
   pub fn get_registry() -> &'static RwLock<AdapterRegistry> {
       static REGISTRY: OnceCell<RwLock<AdapterRegistry>> = OnceCell::new();
       REGISTRY.get_or_init(|| {
           let mut registry = AdapterRegistry::new();
           registry.register("piper", Box::new(PiperAdapter::new()));
           registry.register("elevenlabs", Box::new(ElevenLabsAdapter::new()));
           RwLock::new(registry)
       })
   }
   ```

### Option 3: Azure/Google Cloud (Paid, API, Good Quality)

Similar to ElevenLabs - implement adapter in Rust, register, update config.

## Per-User Voice Preferences (Future)

Allow users to choose their preferred TTS:

```typescript
export interface UserVoicePreferences {
  userId: string;
  preferredTTSAdapter?: TTSAdapter;
  preferredVoice?: string;
  speechRate?: number;  // 0.5-2.0
}

const voice = getVoiceService();
const result = await voice.synthesizeSpeech({
  text: "Hello",
  userId: personaId,  // VoiceService looks up user preferences
});
```

## Testing

### Direct gRPC Test
```bash
node scripts/test-grpc-tts.mjs
# Tests: Rust gRPC → TTS → WAV file
```

### End-to-End Test
```bash
node scripts/test-persona-voice-e2e.mjs
# Tests: Full pipeline including i16 conversion
```

### Live Call Test
1. Open browser to http://localhost:9000
2. Start voice call with a user
3. Speak: "Hey Teacher, what is AI?"
4. Teacher AI should respond with synthesized voice

## Architecture Benefits

1. **Adaptable**: Swap TTS engines by changing one config line
2. **Fallback**: Automatic fallback if primary TTS fails
3. **Type-safe**: Full TypeScript types throughout
4. **Off-main-thread**: All heavy TTS work in Rust workers
5. **Real-time**: Fast enough for live conversations (0.13x RT factor)
6. **Pattern consistency**: Mirrors LLM inference architecture

## File Locations

```
system/voice/
├── shared/
│   └── VoiceConfig.ts          # Adapter configuration
├── server/
│   ├── VoiceService.ts         # High-level TTS API
│   ├── VoiceOrchestrator.ts    # Turn arbitration
│   └── AIAudioBridge.ts        # Call integration

commands/voice/synthesize/
├── shared/VoiceSynthesizeTypes.ts
└── server/VoiceSynthesizeServerCommand.ts  # gRPC bridge

workers/streaming-core/src/
├── tts/
│   ├── mod.rs                  # TTS registry
│   ├── piper.rs                # Piper adapter
│   └── phonemizer.rs           # Text → phonemes
├── mixer.rs                    # Audio mixing
├── voice_service.rs            # gRPC service
└── call_server.rs              # WebSocket call handling

scripts/
├── test-grpc-tts.mjs           # Direct TTS test
└── test-persona-voice-e2e.mjs  # Full pipeline test
```

## Next Steps

1. **Improve quality**: Switch to Kokoro or ElevenLabs
2. **Per-user voices**: Let users choose TTS preferences
3. **Streaming synthesis**: Stream audio chunks as they're generated (not batched)
4. **Voice cloning**: Use F5-TTS or XTTS-v2 for custom voices
5. **Multi-lingual**: Support languages beyond English

---

**Status**: ✅ Working! PersonaUsers can speak in voice calls.
**Quality**: Basic (Piper TTS) - ready to upgrade to Kokoro or ElevenLabs.
**Performance**: 0.13x realtime factor - fast enough for live conversations.
