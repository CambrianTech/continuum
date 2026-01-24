# Models Setup Guide

Complete guide for downloading and managing ML models.

## Required Models

### Silero VAD (~1.8MB) - REQUIRED

**Purpose**: Voice activity detection (speech vs silence/noise)

```bash
cd workers/streaming-core
mkdir -p models
curl -L -o models/silero_vad.onnx \
  https://github.com/snakers4/silero-vad/raw/master/files/silero_vad.onnx
```

**Verify**:
```bash
ls -lh models/silero_vad.onnx
# Should be ~1.8MB

sha256sum models/silero_vad.onnx
# Expected: a3b...  (verify checksum)
```

## Optional Models

### Whisper STT

**Purpose**: Speech-to-text transcription

**Model sizes**:
| Model | Size | Speed | Accuracy | Recommended |
|-------|------|-------|----------|-------------|
| tiny.en | 75MB | ~10x real-time | Lower | Testing only |
| base.en | 140MB | ~3x real-time | Good | **Production** ✅ |
| small.en | 460MB | ~1.5x real-time | Better | High accuracy |
| medium.en | 1.5GB | ~1x real-time | Best | GPU recommended |

**Download (base model)**:
```bash
curl -L -o models/ggml-base.en.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin
```

**Multilingual support**:
```bash
# For non-English:
curl -L -o models/ggml-base.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.bin
```

### Piper TTS

**Purpose**: Text-to-speech synthesis

**Voice quality**:
| Quality | Size | Speed | Naturalness |
|---------|------|-------|-------------|
| low | ~5MB | ~5x real-time | Acceptable |
| medium | ~20MB | ~2x real-time | **Good** ✅ |
| high | ~60MB | ~1x real-time | Best |

**Download (medium quality, US English)**:
```bash
# Voice model
curl -L -o models/en_US-lessac-medium.onnx \
  https://github.com/rhasspy/piper/releases/download/v0.0.2/en_US-lessac-medium.onnx

# Voice config
curl -L -o models/en_US-lessac-medium.onnx.json \
  https://github.com/rhasspy/piper/releases/download/v0.0.2/en_US-lessac-medium.onnx.json
```

**Other voices**:
- Browse: https://github.com/rhasspy/piper/releases
- Female voices: amy, kimberly, kathleen
- Male voices: danny, ryan, lessac
- Languages: en_US, en_GB, es_ES, fr_FR, de_DE, etc.

## Model Storage

**Recommended structure**:
```
workers/streaming-core/models/
├── silero_vad.onnx              # VAD (required)
├── ggml-base.en.bin             # STT (optional)
├── en_US-lessac-medium.onnx     # TTS voice (optional)
└── en_US-lessac-medium.onnx.json # TTS config (optional)
```

**Disk space**:
- Minimal (VAD only): ~2MB
- Basic (VAD + STT + TTS): ~162MB
- Full (all models): ~2.5GB

## Model Updates

**Check for updates**:
```bash
# Silero VAD
# Latest: https://github.com/snakers4/silero-vad/releases

# Whisper
# Latest: https://github.com/ggerganov/whisper.cpp/releases

# Piper
# Latest: https://github.com/rhasspy/piper/releases
```

**Update procedure**:
1. Download new model to `models/model-name.new`
2. Test with integration tests
3. If tests pass: `mv models/model-name.new models/model-name`
4. If tests fail: keep old model, investigate

## Model Verification

**After downloading**:
```bash
cd workers/streaming-core

# Check all models present
./scripts/verify_models.sh

# Or manually:
ls -lh models/
# Verify sizes match documentation
```

**Integration test**:
```bash
# Test VAD
cargo test --test vad_production -- --ignored

# Test STT
cargo test stt -- --ignored

# Test TTS
cargo test tts -- --ignored

# Test full pipeline
cargo test --test end_to_end_voice_pipeline -- --ignored
```

## Troubleshooting

### Download fails

**Symptom**: curl returns errors or HTML

**Solutions**:
```bash
# Use alternative mirror
curl -L -o models/silero_vad.onnx \
  https://github.com/snakers4/silero-vad/releases/download/v3.1/silero_vad.onnx

# Or download manually and copy to models/
```

### Model not loaded

**Symptom**: "ModelNotLoaded: ..." error

**Check**:
```bash
# Verify file exists and is not corrupted
ls -lh models/silero_vad.onnx
file models/silero_vad.onnx
# Should show: "models/silero_vad.onnx: data"

# Check permissions
chmod 644 models/*.onnx models/*.bin
```

### Wrong model version

**Symptom**: Unexpected behavior or crashes

**Solution**:
```bash
# Delete and re-download
rm models/silero_vad.onnx
curl -L -o models/silero_vad.onnx \
  https://github.com/snakers4/silero-vad/raw/master/files/silero_vad.onnx
```

## Model Licensing

**Silero VAD**: MIT License (commercial use OK)
**Whisper**: MIT License (commercial use OK)
**Piper**: MIT License (commercial use OK)

All models are free for commercial and non-commercial use.

## Custom Models

### Using custom Whisper models

```rust
// Specify custom model path
let stt = WhisperSTT::with_model_path("models/my-custom-whisper.bin")?;
```

### Using custom Piper voices

```rust
// Specify custom voice
let tts = PiperTTS::with_voice("models/my-custom-voice.onnx")?;
```

## Automated Setup Script

```bash
#!/bin/bash
# setup_models.sh - Download all models

set -e

MODELS_DIR="workers/streaming-core/models"
mkdir -p "$MODELS_DIR"
cd "$MODELS_DIR"

echo "Downloading models..."

# Silero VAD (required)
if [ ! -f "silero_vad.onnx" ]; then
    curl -L -o silero_vad.onnx \
      https://github.com/snakers4/silero-vad/raw/master/files/silero_vad.onnx
    echo "✓ Silero VAD"
fi

# Whisper STT (optional)
if [ ! -f "ggml-base.en.bin" ]; then
    curl -L -o ggml-base.en.bin \
      https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin
    echo "✓ Whisper STT"
fi

# Piper TTS (optional)
if [ ! -f "en_US-lessac-medium.onnx" ]; then
    curl -L -o en_US-lessac-medium.onnx \
      https://github.com/rhasspy/piper/releases/download/v0.0.2/en_US-lessac-medium.onnx
    curl -L -o en_US-lessac-medium.onnx.json \
      https://github.com/rhasspy/piper/releases/download/v0.0.2/en_US-lessac-medium.onnx.json
    echo "✓ Piper TTS"
fi

echo ""
echo "✅ All models downloaded"
ls -lh
```

**Usage**:
```bash
chmod +x scripts/setup_models.sh
./scripts/setup_models.sh
```
