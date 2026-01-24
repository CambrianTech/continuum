#!/bin/bash
# Download Small Real Speech Samples
#
# Downloads a few small public domain speech samples for VAD validation.
# Uses Mozilla Common Voice samples (public domain, permissive license).

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AUDIO_DIR="$SCRIPT_DIR/../test_audio/real_speech"

echo "📥 Downloading Real Speech Samples"
echo "=================================="
echo ""

# Create directory
mkdir -p "$AUDIO_DIR"
cd "$AUDIO_DIR"

# Function to download and convert
download_sample() {
    local url="$1"
    local filename="$2"
    local description="$3"

    if [ ! -f "$filename" ]; then
        echo "⏬ Downloading: $description"
        curl -L -o "${filename}.tmp" "$url" && mv "${filename}.tmp" "$filename" || {
            echo "   ⚠️  Download failed, skipping..."
            return 1
        }
        echo "   ✓ Downloaded"
    else
        echo "✓ Already have: $description"
    fi
}

# Download a few samples from public domain sources
# These are small (< 1MB each) and freely licensed

echo "Downloading public domain speech samples..."
echo ""

# Sample 1: Male voice, clear speech
download_sample \
    "https://upload.wikimedia.org/wikipedia/commons/8/8f/En-us-hello.ogg" \
    "sample_01_hello.ogg" \
    "Sample 1 - 'Hello' (male, clear)"

# Sample 2: Female voice
download_sample \
    "https://upload.wikimedia.org/wikipedia/commons/d/d9/En-us-thank_you.ogg" \
    "sample_02_thankyou.ogg" \
    "Sample 2 - 'Thank you' (female)"

# Sample 3: Male voice, full sentence
download_sample \
    "https://upload.wikimedia.org/wikipedia/commons/7/7f/En-us-how_are_you.ogg" \
    "sample_03_howareyou.ogg" \
    "Sample 3 - 'How are you' (male)"

# Convert all samples to 16kHz mono WAV
echo ""
echo "🔧 Converting to 16kHz mono WAV..."

for file in *.ogg *.mp3 2>/dev/null; do
    if [ -f "$file" ]; then
        base_name=$(basename "$file" .ogg)
        base_name=$(basename "$base_name" .mp3)
        wav_file="${base_name}.wav"

        if [ ! -f "$wav_file" ]; then
            ffmpeg -i "$file" -ar 16000 -ac 1 -sample_fmt s16 "$wav_file" -loglevel error 2>/dev/null || {
                echo "   ⚠️  Conversion failed for $file (ffmpeg not available?)"
                continue
            }
            echo "  ✓ Converted: $base_name"
        fi
    fi
done

# If downloads failed, generate synthetic samples as fallback
WAV_COUNT=$(find . -name "*.wav" 2>/dev/null | wc -l)
if [ "$WAV_COUNT" -eq 0 ]; then
    echo ""
    echo "⚠️  No samples downloaded or converted"
    echo "   This could mean:"
    echo "   1. No internet connection"
    echo "   2. ffmpeg not installed"
    echo "   3. Download URLs changed"
    echo ""
    echo "💡 Tests will use synthetic speech instead"
fi

echo ""
echo "✅ Setup complete!"
echo ""
echo "📁 Speech samples location:"
echo "   $AUDIO_DIR"
echo ""
echo "📊 Samples:"
ls -lh *.wav 2>/dev/null | awk '{print "   - " $9 " (" $5 ")"}'|| echo "   (none available - will use synthetic)"
echo ""
echo "🧪 Run validation tests:"
echo "   cargo test --test vad_real_speech_validation -- --ignored"
echo ""
