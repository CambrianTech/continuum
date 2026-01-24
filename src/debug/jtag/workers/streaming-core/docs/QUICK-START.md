# Quick Start Guide

Get the VAD system running in 5 minutes.

## Prerequisites

```bash
# Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build tools (Ubuntu/Debian)
sudo apt-get install build-essential pkg-config libssl-dev ffmpeg
```

## Download Models

```bash
cd workers/streaming-core
mkdir -p models

# Silero VAD (~1.8MB) - REQUIRED
curl -L -o models/silero_vad.onnx \
  https://github.com/snakers4/silero-vad/raw/master/files/silero_vad.onnx
```

## Build and Test

```bash
cd src/debug/jtag/workers
cargo build --release        # ~5-10 min first time
cargo test --lib             # Unit tests (no models needed)
cargo test --test vad_production -- --ignored  # Integration tests
```

## Basic Usage

```rust
use streaming_core::mixer::{AudioMixer, ParticipantStream};
use streaming_core::Handle;

// Create mixer
let mut mixer = AudioMixer::default_voice();

// Add participant
let handle = Handle::new();
let mut participant = ParticipantStream::new(
    handle,
    "user-1".into(),
    "Alice".into(),
);

// Initialize VAD
participant.initialize_vad().await?;
mixer.add_participant(participant);

// Process audio
let result = mixer.push_audio(&handle, audio_frame);
if result.speech_ended {
    // Send to transcription
    transcribe(result.speech_samples.unwrap()).await?;
}
```

## What You Get

✅ **ProductionVAD**:
- Two-stage detection (WebRTC → Silero)
- 5400x faster on silence
- 80% noise rejection
- Complete sentence buffering

✅ **Audio Mixer**:
- Multi-participant support
- Mix-minus (echo cancellation)
- VAD integrated

✅ **TTS/STT Adapters**:
- Piper, Kokoro (TTS)
- Whisper (STT)
- Runtime swappable

## Next Steps

1. **Configure for your environment**: See [CONFIGURATION-GUIDE.md](CONFIGURATION-GUIDE.md)
2. **Download optional models**: See [MODELS-SETUP.md](MODELS-SETUP.md)
3. **Tune performance**: See [PERFORMANCE-TUNING.md](PERFORMANCE-TUNING.md)
4. **Troubleshoot issues**: See [TROUBLESHOOTING.md](TROUBLESHOOTING.md)

## Full Documentation

- [VAD-FINAL-SUMMARY.md](../../../docs/VAD-FINAL-SUMMARY.md) - Complete VAD overview
- [MIXER-VAD-INTEGRATION.md](MIXER-VAD-INTEGRATION.md) - Mixer integration details
- [PRODUCTION-DEPLOYMENT.md](PRODUCTION-DEPLOYMENT.md) - Full deployment guide
