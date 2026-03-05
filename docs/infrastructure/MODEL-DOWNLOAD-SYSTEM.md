# Automated Model Download System

## Problem

The repo required manual downloads of ML models (Whisper STT, Kokoro TTS) with instructions scattered across warnings and error messages. This violated the fundamental principle: **users should just `npm install` and have everything work**.

## Solution

Created a fully automated model download system that runs during `npm install` and `npm start`.

---

## Components

### 1. Model Manifest (`workers/streaming-core/models.json`)

Centralized registry of all required models:

```json
{
  "models": [
    {
      "name": "Whisper Base English",
      "type": "stt",
      "required": true,
      "path": "models/whisper/ggml-base.en.bin",
      "url": "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
      "size": "141MB",
      "description": "Speech-to-text model for English transcription"
    }
  ]
}
```

### 2. Download Script (`scripts/download-voice-models.sh`)

Bash script that:
- Checks if models already exist (skip if present)
- Downloads from HuggingFace using curl/wget
- Shows progress bar during download
- Handles errors gracefully
- Supports optional models (Kokoro TTS)

**Key Features:**
- Idempotent (safe to run multiple times)
- Cross-platform (works on macOS/Linux)
- Clear error messages with manual download instructions
- Color-coded output (green ✅ / yellow ⚠️ / red ❌)

### 3. TypeScript Downloader (`scripts/download-models.ts`)

Alternative TypeScript implementation with:
- Redirect following (302 redirects)
- Progress percentage
- SHA256 verification (future)
- Better error handling
- JSON manifest parsing

**Usage:**
```bash
npx tsx scripts/download-models.ts
npx tsx scripts/download-models.ts --force  # Re-download existing
```

### 4. npm Lifecycle Hooks

**Automatic triggers:**

```json
{
  "scripts": {
    "postinstall": "npm run worker:models",
    "prebuild": "... && npm run worker:models && ...",
    "worker:models": "./scripts/download-voice-models.sh",
    "worker:start": "./scripts/download-voice-models.sh && ..."
  }
}
```

**When models download:**
- `npm install` → Downloads missing models
- `npm start` → Verifies models exist before starting workers
- `npm run prebuild` → Ensures models exist before build
- `npm run worker:models` → Manual trigger

---

## Model Locations

```
jtag/models/  (gitignored)
├── whisper/
│   └── ggml-base.en.bin    (141MB - REQUIRED)
└── kokoro/
    ├── kokoro-v0_19.onnx   (82MB - OPTIONAL)
    └── voices.json          (2KB - OPTIONAL)
```

**Why `models/` not `workers/streaming-core/models/`?**
- Worker binary runs from `jtag/` root, not `workers/streaming-core/`
- Keeps models outside of Cargo workspace
- Easier to share between multiple workers if needed

---

## Model Details

### Whisper Base English (REQUIRED)

**Purpose**: Speech-to-text transcription during voice calls

**Details:**
- Size: 141 MB
- Source: [ggerganov/whisper.cpp](https://huggingface.co/ggerganov/whisper.cpp/tree/main)
- License: MIT
- Language: English only (faster than multilingual)
- Accuracy: ~95% for clear speech

**What happens without it:**
```rust
WARN STT adapter not initialized - skipping transcription
```
- Voice calls work but have no transcription
- VAD (voice activity detection) still works
- Audio mixing works
- Just no text output from speech

### Kokoro TTS (OPTIONAL)

**Purpose**: Text-to-speech synthesis for AI responses

**Details:**
- Size: 327 MB (PyTorch) → 82 MB (ONNX after conversion)
- Source: [hexgrad/Kokoro-82M](https://huggingface.co/hexgrad/Kokoro-82M)
- License: Check HuggingFace repo
- Voices: Multiple (American, British, male, female)
- Format: PyTorch (`.pth`) → ONNX (`.onnx`) via conversion script

**Why optional:**
- Has graceful fallback (SilenceTTS)
- Requires Python + ML dependencies (torch, onnx, huggingface-hub)
- Conversion takes ~30 seconds
- Voice synthesis is "nice to have", not required for core functionality

**Automatic Conversion**:
- `scripts/convert-kokoro-to-onnx.py` downloads PyTorch weights and converts to ONNX
- Called automatically by `scripts/download-voice-models.sh`
- Fails gracefully if Python dependencies missing

**What happens without it:**
```rust
WARN TTS adapter not available: Model not loaded
WARN TTS will use fallback (silence)
```
- AI responses are silent (no voice synthesis)
- Calls still work normally otherwise
- Transcription of your speech still works

---

## Verification

### Check Model Status

```bash
# List downloaded models
ls -lh models/whisper/ models/kokoro/

# Expected output:
# models/whisper/ggml-base.en.bin  (141MB)
# models/kokoro/kokoro-v0_19.onnx   (82MB) - optional
# models/kokoro/voices.json         (2KB)  - optional
```

### Check Runtime Status

```bash
# Start workers and grep logs
npm run worker:start 2>&1 | grep -i model

# Expected output:
# ✅ Whisper model already exists
# ⚠️  Kokoro TTS model not found (optional - using fallback)
```

### Verify Whisper Loaded

```bash
# Check streaming-core logs for Whisper initialization
ps aux | grep streaming-core  # Get PID
tail -f /tmp/streaming-core-new.log | grep -i whisper

# Expected:
# [INFO] Loading Whisper model from: "models/whisper/ggml-base.en.bin"
# whisper_model_load: model size = 147.37 MB
# [INFO] Whisper model loaded successfully
```

---

## Troubleshooting

### "Whisper model not found" error

**Symptom:**
```
WARN Whisper model not found at "models/whisper/ggml-base.en.bin"
WARN STT adapter not initialized - skipping transcription
```

**Fix:**
```bash
# Re-download models
npm run worker:models

# Or manual download
mkdir -p models/whisper
curl -L https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin \
  -o models/whisper/ggml-base.en.bin
```

### Download fails with SSL error

**Symptom:**
```
curl: (60) SSL certificate problem: certificate has expired
```

**Fix:**
```bash
# Update certificates (macOS)
brew upgrade openssl curl

# Or bypass SSL (NOT RECOMMENDED for production)
curl -k -L <url> -o <output>
```

### Download interrupted / corrupted file

**Symptom:**
- File exists but is < 1 MB
- Model load fails with "Invalid format" error

**Fix:**
```bash
# Remove corrupted file
rm models/whisper/ggml-base.en.bin

# Re-download
npm run worker:models --force
```

### Model downloads every time

**Symptom:**
- `npm start` re-downloads models even though they exist

**Cause:**
- Script checks if file exists, not if download is valid
- File might be 0 bytes or corrupted

**Fix:**
```bash
# Check file size
ls -lh models/whisper/ggml-base.en.bin

# Should be ~141MB, not 0 bytes or <1MB
# If small, remove and re-download:
rm models/whisper/ggml-base.en.bin
npm run worker:models
```

---

## Architecture Decisions

### Why HuggingFace Instead of Git LFS?

**Rejected: Git LFS**
- Costs money for bandwidth
- Requires Git LFS installation
- Complicates clone for users
- Bandwidth limits hit quickly

**Chosen: HuggingFace Direct Download**
- Free, unlimited bandwidth
- No special tools required (curl/wget)
- Official model source
- Fast CDN

### Why Bash Script Instead of npm Package?

**Rejected: npm package downloader**
- Adds dependency (node-fetch, etc.)
- Requires Node.js to be working
- More complex error handling

**Chosen: Bash + curl/wget**
- Minimal dependencies (curl/wget pre-installed)
- Works even if Node.js has issues
- Simple, readable, debuggable
- Cross-platform (macOS/Linux)

### Why .gitignore models/?

**Reason:**
- Models are 100+ MB each (too large for Git)
- Git repo stays small (~50MB instead of ~350MB)
- Faster clones for users
- Models downloaded automatically anyway

**Alternative considered:**
- Check in models directly
- **Rejected**: Makes repo huge, slow clones

---

## Future Enhancements

### 1. SHA256 Verification

Add checksums to `models.json`:

```json
{
  "sha256": "abc123...",
  "url": "https://..."
}
```

Verify after download to ensure integrity.

### 2. Model Version Pinning

```json
{
  "version": "v1.5.4",
  "url": "https://.../v1.5.4/model.bin"
}
```

Lock to specific versions to avoid breaking changes.

### 3. Differential Updates

Only download changed models:

```bash
curl -I <url>  # Get Content-Length
compare with local file size
skip if identical
```

### 4. Parallel Downloads

Download multiple models concurrently:

```bash
curl <whisper_url> -o whisper.bin &
curl <kokoro_url> -o kokoro.onnx &
wait
```

### 5. Progress in TypeScript

Use TypeScript downloader for better UX:

```typescript
[=====     ] 50% Whisper (70MB / 141MB)
[===       ] 30% Kokoro (25MB / 82MB)
```

### 6. Runtime Auto-Download

If model missing at runtime, download automatically:

```rust
if !model_exists() {
    warn!("Model not found, downloading...");
    download_model()?;
}
```

---

## Summary

**Before:**
1. Clone repo
2. Read error message
3. Manually download model
4. Figure out where to place it
5. Hope it works

**After:**
1. Clone repo
2. `npm install` (models download automatically)
3. `npm start` (everything works)

**Key Principle Achieved:**
> Users should just `npm install` and have everything work.

---

## Files Modified/Created

| File | Status | Purpose |
|------|--------|---------|
| `scripts/download-voice-models.sh` | **Modified** | Added Kokoro auto-download |
| `scripts/download-models.ts` | **Created** | TypeScript alternative |
| `workers/streaming-core/models.json` | **Created** | Model manifest |
| `models/README.md` | **Created** | User documentation |
| `docs/MODEL-DOWNLOAD-SYSTEM.md` | **Created** | System documentation |
| `package.json` | **Modified** | Added `postinstall` hook |

---

**Result:** ✅ Repo is now fully self-contained. No manual downloads required.
