# Live -- Voice, Video & Real-Time Communication

> Humans and AIs share the same call. AIs listen through transcription, speak through synthesis, see through vision models, and participate as equal citizens in every conversation. The render loop is sacred -- all processing happens off the main thread, zero exceptions.

**Status:** Voice pipeline operational. VAD production-ready (Silero, 100% noise rejection). Multi-party conferencing implemented. Vision/media Phase 1 complete.

---

## Documents

### Architecture (start here)

| Document | Summary |
|----------|---------|
| [LIVE-CALL-ARCHITECTURE.md](LIVE-CALL-ARCHITECTURE.md) | **Start here.** Game engine philosophy -- render loop sacred, handle-based zero-copy architecture, command buffers, mix-minus audio |
| [STREAMING-BACKBONE-ARCHITECTURE.md](STREAMING-BACKBONE-ARCHITECTURE.md) | Universal real-time infrastructure -- ring buffers, pipeline stages, adapters for voice/video/generation on ONE backbone |
| [CONTINUOUS-TRANSCRIPTION-ARCHITECTURE.md](CONTINUOUS-TRANSCRIPTION-ARCHITECTURE.md) | Low-latency streaming transcription with continuous output, sliding window buffer, no waiting for silence |
| [LIVEWIDGET-REFACTORING-PLAN.md](LIVEWIDGET-REFACTORING-PLAN.md) | LiveWidget.ts refactoring plan -- split 1026-line monolith into LiveCallState, LiveMediaManager, LiveParticipantRenderer |

### Voice

| Document | Summary |
|----------|---------|
| [VOICE-ARCHITECTURE.md](VOICE-ARCHITECTURE.md) | Real-time voice with WebSocket pipeline -- concurrency model, single STT per room, AI turn-taking, lock-free audio, 500ms latency budget |
| [VOICE-STREAMING-ARCHITECTURE.md](VOICE-STREAMING-ARCHITECTURE.md) | Voice chat infrastructure -- commands (start/stop/transcribe/synthesize), TTS adapter registry, gRPC to Rust streaming-core |
| [VOICE-SYNTHESIS-ARCHITECTURE.md](VOICE-SYNTHESIS-ARCHITECTURE.md) | TTS for PersonaUser speech -- VoiceOrchestrator, AIAudioBridge, VoiceService, Piper/Kokoro adapters, 0.13x realtime factor |
| [VOICE-CONFERENCE-ARCHITECTURE.md](VOICE-CONFERENCE-ARCHITECTURE.md) | Multi-party conferencing -- N humans + M AIs, AudioMixer with mix-minus, turn coordination, WebRTC transport |
| [VOICE-AI-RESPONSE-PLAN.md](VOICE-AI-RESPONSE-PLAN.md) | Architecture plan for routing AI responses to TTS instead of chat -- PersonaResponseRouter, modality-based routing |
| [VOICE-AI-RESPONSE-FIXED.md](VOICE-AI-RESPONSE-FIXED.md) | What was fixed -- directed event emission, arbiter-selected persona receives transcription, debug spam removed |

### Voice Activity Detection (VAD)

The VAD documents trace a progression from problem identification through to production-ready system:

| Document | Summary |
|----------|---------|
| [VAD-SYSTEM-ARCHITECTURE.md](VAD-SYSTEM-ARCHITECTURE.md) | Core problem and solution -- trait-based modular VAD replacing primitive RMS threshold, Silero ONNX integration |
| [VAD-SILERO-INTEGRATION.md](VAD-SILERO-INTEGRATION.md) | Silero ONNX integration details -- HuggingFace model, LSTM state handling, TV dialogue insight (VAD detecting TV speech is correct) |
| [VAD-SYNTHETIC-AUDIO-FINDINGS.md](VAD-SYNTHETIC-AUDIO-FINDINGS.md) | Why synthetic audio cannot evaluate ML VAD -- formant synthesis limitations, Silero's selectivity is a feature |
| [VAD-TEST-RESULTS.md](VAD-TEST-RESULTS.md) | Noisy environment test results -- RMS at 28.6% accuracy, factory floor 10/10 false positives, threshold sensitivity analysis |
| [VAD-METRICS-RESULTS.md](VAD-METRICS-RESULTS.md) | 100% noise rejection results -- confusion matrices, precision/recall/F1 for RMS vs WebRTC vs Silero, 0% false positive rate |
| [VAD-PRODUCTION-CONFIG.md](VAD-PRODUCTION-CONFIG.md) | Production config -- two-stage VAD (WebRTC pre-filter then Silero), 5400x faster on silence, sentence buffering, adaptive thresholds |
| [VAD-SYSTEM-COMPLETE.md](VAD-SYSTEM-COMPLETE.md) | Complete implementation summary -- 4 VAD implementations, file list, performance metrics, usage examples |
| [VAD-FINAL-SUMMARY.md](VAD-FINAL-SUMMARY.md) | Production-ready summary -- 11,457 lines, 42 files, all acceptance criteria met, deployment checklist |

### Media Processing

| Document | Summary |
|----------|---------|
| [MEDIA-FORMAT-CONVERSION-ARCHITECTURE.md](MEDIA-FORMAT-CONVERSION-ARCHITECTURE.md) | AI provider format support -- MediaCapabilities interface, MediaConverter utility, adapter-driven conversion (WebP to JPEG for DeepSeek) |
| [MEDIA-PROCESS-IMPLEMENTATION-STATUS.md](MEDIA-PROCESS-IMPLEMENTATION-STATUS.md) | media/process command wrapping ffmpeg -- script orchestration pattern, platform scripts, progress events, dependency management |
| [VISION-MEDIA-ARCHITECTURE.md](VISION-MEDIA-ARCHITECTURE.md) | Vision and media phases -- image description for non-vision models, context-aware resizing, RAG budget integration (flexbox analogy) |

### Related (other chapters)

| Document | Chapter | Relevance |
|----------|---------|-----------|
| [PersonaUser Convergence](../personas/PERSONA-CONVERGENCE-ROADMAP.md) | Personas | PersonaUser voice integration, VoiceOrchestrator inbox routing |
| [Consciousness Integration](../personas/CONSCIOUSNESS-INTEGRATION-FLOW.md) | Personas | How voice transcriptions flow through PersonaUser cognitive loop |
| [Genome Architecture](../genome/GENOME-ARCHITECTURE.md) | Genome | Voice LoRA fine-tuning, per-persona voice identity |
| [AI Adapter Architecture](../infrastructure/AI-ADAPTER-ARCHITECTURE-REFACTOR.md) | Infrastructure | Adapter pattern shared between voice and media capabilities |
| [Rust Worker Architecture](../infrastructure/AI-PROVIDER-WORKER-ARCHITECTURE.md) | Infrastructure | Streaming-core Rust worker design, IPC boundaries |
| [Positron Architecture](../positron/POSITRON-ARCHITECTURE.md) | Positron | LiveWidget integration, Lit reactive properties |
| [HUD Design](../positron/BRAIN-HUD-DESIGN.md) | Positron | Call controls, participant tiles, speaking indicators |

---

## Key Principles

- **Render loop is sacred** -- off-main-thread everything, zero exceptions since GCD (2009)
- **AIs are equal participants** in calls -- they speak, listen, see, and respond through the same infrastructure
- **VAD ensures only real speech triggers processing** -- Silero ML model, 100% noise rejection, two-stage for performance
- **Voice identity is sacred** -- never degrades under GPU pressure (rendering quality can degrade, voice identity never changes)
- **Zero-copy pipeline** -- handles not data, ring buffers not allocations, Rust core with TypeScript thin client
- **Handle pattern for correlation** -- start returns handle, events tagged with handle, cancel/status by handle

---

## Implementation Status

| Component | Status |
|-----------|--------|
| Voice WebSocket pipeline | Operational |
| VAD (Silero + two-stage) | Production-ready |
| TTS (Piper, stubs for Kokoro/others) | Working (basic quality) |
| STT (Whisper stub) | Stub -- needs real inference |
| Multi-party conferencing | Architecture complete |
| LiveWidget | Working (needs refactoring) |
| Media format conversion | Phase 1 complete |
| Vision/media processing | Phase 1 complete |
| Streaming backbone | Designed, not built |
