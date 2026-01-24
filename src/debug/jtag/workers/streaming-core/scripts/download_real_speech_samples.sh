#!/bin/bash
# Download Real Speech Samples for VAD Validation
#
# Downloads a small subset of LibriSpeech for testing with actual human voice.
# This validates that ProductionVAD works with real speech (not just formant synthesis).

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AUDIO_DIR="$SCRIPT_DIR/../test_audio/real_speech"
LIBRISPEECH_URL="https://www.openslr.org/resources/12"

echo "📥 Downloading Real Speech Samples"
echo "=================================="
echo ""

# Create directory
mkdir -p "$AUDIO_DIR"
cd "$AUDIO_DIR"

# Download dev-clean-2 (smallest test set, 337MB)
# Contains clean speech from multiple speakers
DATASET="dev-clean-2"
ARCHIVE="${DATASET}.tar.gz"

if [ ! -f "$ARCHIVE" ]; then
    echo "⏬ Downloading LibriSpeech ${DATASET} (~337MB)..."
    echo "   This may take a few minutes..."
    curl -L -o "$ARCHIVE" "${LIBRISPEECH_URL}/${ARCHIVE}" || {
        echo "❌ Download failed. Trying alternative source..."
        # Try alternative source
        curl -L -o "$ARCHIVE" "https://us.openslr.org/resources/12/${ARCHIVE}"
    }
else
    echo "✓ Archive already downloaded"
fi

# Extract
if [ ! -d "LibriSpeech" ]; then
    echo ""
    echo "📦 Extracting archive..."
    tar -xzf "$ARCHIVE"
    echo "✓ Extracted successfully"
else
    echo "✓ Already extracted"
fi

# Convert to 16kHz mono WAV for VAD testing
echo ""
echo "🔧 Converting samples to 16kHz mono WAV..."

# Find first 10 FLAC files and convert them
CONVERTED=0
find LibriSpeech -name "*.flac" | head -10 | while read -r flac_file; do
    base_name=$(basename "$flac_file" .flac)
    wav_file="${base_name}.wav"

    if [ ! -f "$wav_file" ]; then
        # Convert using ffmpeg: FLAC → 16kHz mono WAV
        ffmpeg -i "$flac_file" -ar 16000 -ac 1 -sample_fmt s16 "$wav_file" -loglevel error
        echo "  ✓ Converted: $base_name"
        CONVERTED=$((CONVERTED + 1))
    fi
done

echo ""
echo "✅ Download and conversion complete!"
echo ""
echo "📁 Real speech samples location:"
echo "   $AUDIO_DIR"
echo ""
echo "📊 Dataset info:"
echo "   - LibriSpeech dev-clean-2 (clean speech)"
echo "   - Multiple speakers"
echo "   - Read audiobooks (clear, natural speech)"
echo "   - 16kHz mono WAV format"
echo ""
echo "🧪 Use these for testing:"
echo "   cargo test --test vad_real_speech_validation -- --ignored"
echo ""
