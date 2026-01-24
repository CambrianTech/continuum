# Voice Activity Detection (VAD) Module

Modular VAD system for distinguishing speech from silence and background noise.

## Problem

**Previous implementation**: Primitive RMS threshold (line 208 of mixer.rs)
```rust
let is_silence = test_utils::is_silence(&samples, 500.0);
```

**Issues**:
- Cannot distinguish speech from TV audio, music, or background noise
- Both speech and TV have similar RMS values (~500-5000)
- Results in unwanted transcriptions of background audio

## Solution

Modular VAD system supporting multiple algorithms:

| Algorithm | Accuracy | Latency | Use Case |
|-----------|----------|---------|----------|
| **Silero VAD** (ML) | High - rejects background noise | ~1ms | Production (default) |
| **RMS Threshold** | Low - any loud audio = speech | <0.1ms | Fallback / debugging |

## Architecture

```
VoiceActivityDetection trait (polymorphic)
├── SileroVAD (ML-based, ONNX Runtime)
│   - Trained on 6000+ hours of speech
│   - Rejects TV, music, background noise
│   - 8ms chunk processing
│
└── RmsThresholdVAD (energy-based, primitive)
    - Fast fallback
    - Cannot reject background noise
    - For debugging/low-latency scenarios
```

## Usage

### Automatic (Recommended)

```rust
use streaming_core::VADFactory;

// Creates Silero if model exists, RMS fallback otherwise
let vad = VADFactory::default();
```

### Manual Selection

```rust
// Create specific VAD
let vad = VADFactory::create("silero")?;  // ML-based
// OR
let vad = VADFactory::create("rms")?;  // Primitive

// Initialize (loads models)
vad.initialize().await?;

// Detect speech in audio frame
let result = vad.detect(&samples).await?;
if result.is_speech && result.confidence > 0.5 {
    // Transcribe this audio
}
```

### Environment Variables

```bash
# Select VAD algorithm (default: auto-detect)
export VAD_ALGORITHM=silero  # or "rms"

# Silero model path (default: models/vad/silero_vad.onnx)
export SILERO_VAD_MODEL=silero_vad.onnx
```

## Setup: Download Silero Model

```bash
# Create models directory
mkdir -p models/vad

# Download Silero VAD ONNX model (~1.8MB)
curl -L https://github.com/snakers4/silero-vad/raw/master/files/silero_vad.onnx \
  -o models/vad/silero_vad.onnx

# Or from HuggingFace
wget https://huggingface.co/snakers4/silero-vad/resolve/main/files/silero_vad.onnx \
  -O models/vad/silero_vad.onnx
```

## How It Works: Silero VAD

1. **LSTM-based neural network** - Maintains state across frames
2. **Probability output** - Returns 0.0-1.0 (not speech to definitely speech)
3. **Threshold** - Default 0.5 (configurable)
4. **Input** - 16kHz mono PCM audio, any chunk size (optimized for 8-32ms)
5. **Output** - Speech probability + updated LSTM state

**Key advantage**: Silero is trained on REAL speech data with background noise, music, TV, etc. It learns what human speech "looks like" in the frequency domain, not just energy levels.

## Performance

**Silero VAD**:
- Inference: ~1ms per 32ms frame (30x real-time)
- Model size: 1.8MB (loads instantly)
- Memory: ~10MB (LSTM state + model weights)
- CPU: ~5% of one core at 16kHz

**RMS Threshold**:
- Inference: <0.1ms (pure math, no model)
- Memory: 0 bytes (no state)
- CPU: negligible

## Testing

```bash
# Unit tests (no model required)
cargo test --package streaming-core vad

# Integration tests (requires Silero model)
cargo test --package streaming-core --release -- --ignored test_silero_inference
```

## Debugging

```bash
# Force RMS threshold (bypass Silero)
export VAD_ALGORITHM=rms
npm start

# Test with different threshold
export RMS_THRESHOLD=1000  # Higher = more permissive (default: 500)
```

## Extending: Add New VAD

To add a new VAD algorithm (e.g., WebRTC VAD, Yamnet, etc.):

1. Create `src/vad/your_vad.rs`
2. Implement `VoiceActivityDetection` trait
3. Add to `VADFactory::create()` match statement
4. Update this README

Example:

```rust
// src/vad/webrtc_vad.rs
use super::{VADError, VADResult, VoiceActivityDetection};
use async_trait::async_trait;

pub struct WebRtcVAD {
    // Your state
}

#[async_trait]
impl VoiceActivityDetection for WebRtcVAD {
    fn name(&self) -> &'static str { "webrtc" }
    fn description(&self) -> &'static str { "Google WebRTC VAD" }

    async fn detect(&self, samples: &[i16]) -> Result<VADResult, VADError> {
        // Your implementation
    }

    // ... other trait methods
}
```

Then add to factory:

```rust
// src/vad/mod.rs
match name {
    "rms" => Ok(Box::new(rms_threshold::RmsThresholdVAD::new())),
    "silero" => Ok(Box::new(silero::SileroVAD::new())),
    "webrtc" => Ok(Box::new(webrtc_vad::WebRtcVAD::new())),  // NEW
    _ => Err(...)
}
```

## References

- **Silero VAD**: https://github.com/snakers4/silero-vad
- **ONNX Runtime**: https://onnxruntime.ai/
- **OpenCV Algorithm Pattern**: CLAUDE.md polymorphism section

## Migration from Old Code

**Before** (mixer.rs line 208):
```rust
let is_silence = test_utils::is_silence(&samples, 500.0);
```

**After**:
```rust
let vad_result = futures::executor::block_on(self.vad.detect(&samples));
let is_silence = !vad_result?.is_speech;
```

**Why modular?**
- Easy to swap algorithms without touching mixer.rs
- Can add new VAD implementations independently
- Runtime selection via environment variables
- Follows CLAUDE.md polymorphism pattern
