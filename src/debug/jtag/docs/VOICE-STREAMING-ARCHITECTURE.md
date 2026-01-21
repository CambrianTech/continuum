# Voice Streaming Architecture

## Overview

Voice chat infrastructure for real-time audio communication with AI personas. Everything is streaming, following the same pattern as vision but for audio.

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| **Commands** | | |
| voice/start | ✅ Complete | Start session, get WebSocket URL |
| voice/stop | ✅ Complete | End session |
| voice/transcribe | ✅ Complete | STT via Rust Whisper (stub impl) |
| voice/synthesize | ✅ Complete | TTS via Rust adapters (stub impl) |
| **TypeScript** | | |
| VoiceGrpcClient | ✅ Complete | gRPC client to Rust worker |
| VoiceChatWidget | ✅ Complete | Browser widget with AudioWorklet |
| VoiceWebSocketHandler | ✅ Complete | Routes audio → STT → Chat → PersonaUser → TTS → audio |
| **Rust Worker** | | |
| voice.proto | ✅ Complete | gRPC service definition |
| VoiceService gRPC | ✅ Complete | Implements all RPC methods |
| TTSAdapterRegistry | ✅ Complete | 5 adapters registered |
| TTS Adapters | 🔧 Stubs | Kokoro, Fish, F5, StyleTTS2, XTTS (stubs return silence) |
| STT (Whisper) | 🔧 Stub | Returns placeholder text |

**Next Steps:**
1. ~~Wire VoiceWebSocketHandler to PersonaUser~~ ✅ Done
2. ~~Test full end-to-end voice conversation~~ ✅ Done (verified: transcribe → chat → synthesize flow works)
3. Implement real Kokoro TTS inference (candle-based, currently returns silence)
4. Implement real Whisper STT inference (candle-whisper, currently returns placeholder)
5. Start the streaming-core Rust worker as part of npm start (currently manual: `cd workers/streaming-core && cargo run`)

## Architecture Philosophy

**Same Pattern as Vision**: Models can support voice natively OR use adapters.
- Native voice: Model handles audio directly (future SOTA models)
- Adapter path: STT → text → model → text → TTS

**TypeScript for Portability, Rust for Speed**:
- Commands (`voice/start`, `voice/transcribe`, `voice/synthesize`) in TypeScript
- Heavy lifting (inference, audio processing) in Rust workers
- gRPC between TypeScript and Rust (port 50052)

## Component Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            BROWSER                                       │
├─────────────────────────────────────────────────────────────────────────┤
│  VoiceChatWidget                                                        │
│  ├── AudioWorklet (capture/playback)                                    │
│  ├── WebSocket connection to server                                     │
│  └── UI controls (mute, speaker selection)                              │
└───────────────────────────────┬─────────────────────────────────────────┘
                                │ WebSocket (audio frames)
                                ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                            SERVER                                        │
├─────────────────────────────────────────────────────────────────────────┤
│  VoiceWebSocketHandler                                                  │
│  ├── Receives audio frames from browser                                 │
│  ├── Calls voice/transcribe command                                     │
│  ├── Routes text to PersonaInbox                                        │
│  ├── Calls voice/synthesize command                                     │
│  └── Streams audio back to browser                                      │
│                                                                         │
│  Commands:                                                              │
│  ├── voice/start    - Start voice session, get WebSocket URL            │
│  ├── voice/stop     - End voice session                                 │
│  ├── voice/transcribe - STT via Rust Whisper                            │
│  └── voice/synthesize - TTS via Rust Kokoro/etc                         │
└───────────────────────────────┬─────────────────────────────────────────┘
                                │ gRPC (port 50052)
                                ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                        RUST STREAMING-CORE                               │
├─────────────────────────────────────────────────────────────────────────┤
│  VoiceService (gRPC)                                                    │
│  ├── Synthesize     - Batch TTS                                         │
│  ├── SynthesizeStream - Streaming TTS                                   │
│  ├── Transcribe     - STT                                               │
│  └── ListAdapters   - Available TTS engines                             │
│                                                                         │
│  TTS Adapters (tts.rs):                                                 │
│  ├── KokoroAdapter      - #1 on TTS Arena (80.9% win rate)              │
│  ├── FishSpeechAdapter  - Natural conversational                        │
│  ├── F5TTSAdapter       - Zero-shot voice cloning                       │
│  ├── StyleTTS2Adapter   - Style transfer                                │
│  └── XTTSv2Adapter      - Multi-lingual                                 │
│                                                                         │
│  STT (Whisper via candle):                                              │
│  └── Models: tiny, base, small, medium, large                           │
└─────────────────────────────────────────────────────────────────────────┘
```

## Voice Flow (Real-time Chat)

```
User speaks → Mic → AudioWorklet → WebSocket → Server
                                                  │
                                                  ▼
                                    voice/transcribe (Whisper STT)
                                                  │
                                                  ▼
                                    Text to PersonaInbox
                                                  │
                                                  ▼
                                    PersonaUser generates response
                                                  │
                                                  ▼
                                    voice/synthesize (Kokoro TTS)
                                                  │
                                                  ▼
                        Server → WebSocket → AudioWorklet → Speaker
```

## Commands

### voice/start

Start a voice chat session.

```bash
./jtag voice/start --room="general"
# Returns: { handle: "abc-123", wsUrl: "ws://localhost:3000/ws/voice?handle=abc-123" }

./jtag voice/start --room="general" --model="llama3.2:3b" --voice="amy"
```

**Parameters:**
- `room` (optional): Room ID or name, defaults to "general"
- `model` (optional): LLM model for responses
- `voice` (optional): TTS voice ID

**Returns:**
- `handle`: Session UUID for correlation
- `wsUrl`: WebSocket URL for audio streaming
- `roomId`: Resolved room ID

### voice/stop

End a voice chat session.

```bash
./jtag voice/stop --handle="abc-123"
```

### voice/transcribe

Transcribe audio to text (STT).

```bash
./jtag voice/transcribe --audio="<base64-pcm-data>"
# Returns: { text: "Hello world", language: "en", confidence: 0.95 }

./jtag voice/transcribe --audio="<base64>" --language="es" --model="small"
```

**Parameters:**
- `audio` (required): Base64-encoded PCM 16-bit, 16kHz mono audio
- `format` (optional): Audio format hint ("pcm16", "wav", "opus")
- `language` (optional): Language hint or "auto" (default: auto-detect)
- `model` (optional): Whisper model size ("tiny", "base", "small", "medium", "large")

**Returns:**
- `text`: Transcribed text
- `language`: Detected language code
- `confidence`: Confidence score (0-1)
- `segments`: Word-level timestamps [{word, start, end}]

### voice/synthesize

Synthesize text to speech (TTS).

```bash
./jtag voice/synthesize --text="Hello, how can I help you?"
# Returns: { audio: "<base64-pcm>", sampleRate: 24000, duration: 1.8, adapter: "kokoro" }

./jtag voice/synthesize --text="Welcome!" --voice="amy" --adapter="kokoro" --speed=1.1
```

**Parameters:**
- `text` (required): Text to synthesize
- `voice` (optional): Voice ID (adapter-specific)
- `adapter` (optional): TTS adapter ("kokoro", "fish-speech", "f5-tts", "styletts2", "xtts-v2")
- `speed` (optional): Speed multiplier (0.5-2.0, default 1.0)
- `sampleRate` (optional): Output sample rate (default: adapter's native rate, usually 24000)
- `format` (optional): Output format ("pcm16", "wav", "opus")
- `stream` (optional): Return streaming handle instead of complete audio

**Returns:**
- `audio`: Base64-encoded audio (if stream=false)
- `handle`: Streaming handle (if stream=true)
- `sampleRate`: Actual sample rate
- `duration`: Audio duration in seconds
- `adapter`: TTS adapter that was used

## TTS Adapters

All adapters follow the `TTSAdapter` trait in `/workers/streaming-core/src/tts.rs`:

| Adapter | Quality Rank | Specialty | Native Sample Rate |
|---------|--------------|-----------|-------------------|
| Kokoro | #1 (80.9%) | Best overall quality | 24000 Hz |
| Fish Speech | Good | Natural conversational | 24000 Hz |
| F5-TTS | Good | Zero-shot voice cloning | 24000 Hz |
| StyleTTS2 | Good | Style transfer | 24000 Hz |
| XTTSv2 | Moderate | Multi-lingual support | 22050 Hz |

Rankings from [TTS Arena](https://huggingface.co/spaces/TTS-Arena/tts-arena-leaderboard) as of January 2026.

### Adapter Trait

```rust
#[async_trait]
pub trait TTSAdapter: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn supports_voice_cloning(&self) -> bool;
    fn available_speakers(&self) -> Vec<String>;
    fn default_sample_rate(&self) -> u32;

    async fn load(&mut self) -> Result<(), TTSError>;
    async fn unload(&mut self) -> Result<(), TTSError>;
    fn is_loaded(&self) -> bool;

    async fn synthesize(&self, text: &str, params: &TTSParams) -> Result<Vec<i16>, TTSError>;
    async fn synthesize_stream(&self, text: &str, params: &TTSParams) -> Result<TTSAudioStream, TTSError>;
}
```

## Integration with PersonaUser

Voice follows the same capability-based routing as vision:

1. **AICapabilityRegistry** tracks `audio-input` and `audio-output` capabilities per model
2. **Models with native voice** (future): Audio routed directly to model
3. **Models without native voice**: STT → text → model → text → TTS

### Capability Flow

```typescript
// Check if model supports native voice
const hasNativeVoice = AICapabilityRegistry.hasCapability(modelId, 'audio-input');

if (hasNativeVoice) {
  // Send audio directly to model
  response = await AIProviderDaemon.generateText({
    content: [{ type: 'audio', data: audioBase64 }],
    // ...
  });
} else {
  // Use adapter path
  const text = await Commands.execute('voice/transcribe', { audio: audioBase64 });
  const response = await PersonaInbox.process(text);
  const audio = await Commands.execute('voice/synthesize', { text: response });
}
```

## Streaming Core Architecture

The Rust worker (`/workers/streaming-core/`) handles all heavy audio processing:

```
/workers/streaming-core/
├── src/
│   ├── lib.rs          # Module exports
│   ├── tts.rs          # TTS adapters (Kokoro, Fish, F5, StyleTTS2, XTTSv2)
│   ├── ws_audio.rs     # WebSocket audio adapters
│   ├── frame.rs        # Audio/Video frame types
│   ├── ring.rs         # Lock-free ring buffers
│   ├── event.rs        # Event bus
│   ├── pipeline.rs     # Processing pipeline
│   └── stage.rs        # Pipeline stages (STT, TTS, VAD, LLM)
├── proto/
│   └── voice.proto     # gRPC service definition
└── Cargo.toml
```

### Zero-Copy Design

- Ring buffers hold data, pass SlotRef (8 bytes)
- GPU textures stay on GPU, pass texture ID
- Only copy at boundaries (encode/decode)

### Pipeline Model

```
InputAdapter -> [VAD] -> [STT] -> [LLM] -> [TTS] -> OutputAdapter
     ↓            ↓        ↓        ↓        ↓           ↓
  EventBus ← ← Events (Started, Progress, FrameReady, Completed)
```

## Files

### Commands
- `/commands/voice/start/` - Start voice session
- `/commands/voice/stop/` - Stop voice session
- `/commands/voice/transcribe/` - STT command
- `/commands/voice/synthesize/` - TTS command

### Client
- `/system/core/services/VoiceGrpcClient.ts` - gRPC client for Rust worker

### Server
- `/system/voice/server/VoiceWebSocketHandler.ts` - WebSocket handler for audio → STT → chat → TTS → audio

### Browser
- `/widgets/voice-chat/VoiceChatWidget.ts` - Browser widget
- `/browser/audio/audio-capture-processor.ts` - AudioWorklet capture
- `/browser/audio/audio-playback-processor.ts` - AudioWorklet playback

### Rust Worker
- `/workers/streaming-core/src/tts.rs` - TTS adapters (Kokoro, Fish, F5, StyleTTS2, XTTSv2)
- `/workers/streaming-core/src/voice_service.rs` - gRPC VoiceService implementation
- `/workers/streaming-core/src/ws_audio.rs` - WebSocket audio handling
- `/workers/streaming-core/proto/voice.proto` - gRPC service definition
- `/workers/streaming-core/src/stage.rs` - Pipeline stages (VAD, STT, TTS, LLM)

## Testing

```bash
# Verify TypeScript compiles
npm run build:ts

# Test TTS adapters (Rust)
cd workers/streaming-core && cargo test tts

# Test voice commands (requires running system)
npm start
./jtag voice/synthesize --text="Hello world"
./jtag voice/transcribe --audio="<base64-pcm-data>"

# Full voice chat flow
./jtag voice/start --room="general"
# Open browser, speak, verify AI responds with voice
./jtag voice/stop --handle="<handle>"
```

## Future Work

1. **Native Voice Models**: As SOTA models add native audio I/O, route directly
2. **Voice Cloning**: Use F5-TTS or XTTSv2 for persona-specific voices
3. **Emotion Detection**: Analyze voice for emotional context in RAG
4. **Multi-speaker**: Track multiple speakers in group calls
5. **Phone Integration**: Twilio/WebRTC adapters for phone-based voice chat
