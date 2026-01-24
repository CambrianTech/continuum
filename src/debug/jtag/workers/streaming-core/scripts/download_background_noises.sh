#!/bin/bash
# Download real background noise samples for VAD testing
#
# Sources: Free, public domain, or Creative Commons licensed audio
# All samples converted to 16kHz mono WAV for consistency

set -e

NOISE_DIR="test_audio/background_noise"
mkdir -p "$NOISE_DIR"

echo "Downloading background noise samples..."

# Function to convert any audio to 16kHz mono WAV
convert_to_wav() {
    local input="$1"
    local output="$2"

    if command -v ffmpeg &> /dev/null; then
        ffmpeg -i "$input" -ar 16000 -ac 1 -sample_fmt s16 "$output" -y 2>/dev/null
        echo "  ✓ Converted: $output"
    else
        echo "  ✗ ffmpeg not found - cannot convert $input"
        return 1
    fi
}

# FreeSFX.co.uk - Royalty-free sound effects
# Note: These URLs are examples - actual download may require browser/API

echo ""
echo "Option 1: Manual Download (Recommended)"
echo "========================================="
echo ""
echo "Download these free background noises manually:"
echo ""
echo "1. Call Center Background:"
echo "   https://freesound.org/people/InspectorJ/sounds/411577/"
echo "   (Office ambiance with phones ringing)"
echo ""
echo "2. Coffee Shop / Restaurant:"
echo "   https://freesound.org/people/InspectorJ/sounds/411581/"
echo "   (People chattering, dishes clinking)"
echo ""
echo "3. Street Traffic:"
echo "   https://freesound.org/people/InspectorJ/sounds/346413/"
echo "   (Cars passing, honking)"
echo ""
echo "4. Construction Site:"
echo "   https://freesound.org/people/InspectorJ/sounds/411579/"
echo "   (Machinery, hammering)"
echo ""
echo "5. Busy Office:"
echo "   https://freesound.org/people/InspectorJ/sounds/346644/"
echo "   (Keyboards, printers, conversations)"
echo ""
echo "After downloading, place MP3/WAV files in: $NOISE_DIR/raw/"
echo "Then run this script again with --convert flag"
echo ""

# Check if raw files exist
if [ -d "$NOISE_DIR/raw" ] && [ "$(ls -A $NOISE_DIR/raw 2>/dev/null)" ]; then
    echo ""
    echo "Converting downloaded files..."
    echo "==============================="
    echo ""

    for file in "$NOISE_DIR/raw"/*; do
        if [ -f "$file" ]; then
            filename=$(basename "$file")
            name="${filename%.*}"
            output="$NOISE_DIR/${name}.wav"

            echo "Converting: $filename"
            if convert_to_wav "$file" "$output"; then
                # Verify it's 16kHz mono
                if command -v ffprobe &> /dev/null; then
                    rate=$(ffprobe -v error -show_entries stream=sample_rate -of default=noprint_wrappers=1:nokey=1 "$output")
                    channels=$(ffprobe -v error -show_entries stream=channels -of default=noprint_wrappers=1:nokey=1 "$output")
                    echo "    Format: ${rate}Hz, ${channels} channel(s)"
                fi
            fi
        fi
    done

    echo ""
    echo "✓ Conversion complete!"
    echo ""
    echo "Files ready for testing in: $NOISE_DIR/"
else
    echo ""
    echo "No raw files found in $NOISE_DIR/raw/"
    echo "Please download samples manually first."
fi

# Alternative: Generate synthetic realistic noise using ffmpeg
echo ""
echo "Option 2: Generate Synthetic Realistic Noise"
echo "============================================="
echo ""

if command -v ffmpeg &> /dev/null; then
    echo "Generating synthetic background noises with ffmpeg..."

    # White noise (already have this, but ffmpeg version is more realistic)
    ffmpeg -f lavfi -i "anoisesrc=duration=10:color=white:sample_rate=16000:amplitude=0.1" \
        -ar 16000 -ac 1 "$NOISE_DIR/white_noise_realistic.wav" -y 2>/dev/null
    echo "  ✓ Generated: white_noise_realistic.wav"

    # Pink noise (more natural, 1/f power spectrum)
    ffmpeg -f lavfi -i "anoisesrc=duration=10:color=pink:sample_rate=16000:amplitude=0.1" \
        -ar 16000 -ac 1 "$NOISE_DIR/pink_noise_realistic.wav" -y 2>/dev/null
    echo "  ✓ Generated: pink_noise_realistic.wav"

    # Brown noise (even more natural, 1/f² spectrum)
    ffmpeg -f lavfi -i "anoisesrc=duration=10:color=brown:sample_rate=16000:amplitude=0.1" \
        -ar 16000 -ac 1 "$NOISE_DIR/brown_noise_realistic.wav" -y 2>/dev/null
    echo "  ✓ Generated: brown_noise_realistic.wav"

    echo ""
    echo "✓ Synthetic noises generated!"
else
    echo "ffmpeg not found - skipping synthetic generation"
fi

echo ""
echo "Summary:"
echo "========"
echo "Background noise samples location: $NOISE_DIR/"
echo ""
echo "Next steps:"
echo "1. Download real samples from URLs above (place in $NOISE_DIR/raw/)"
echo "2. Run: ./scripts/download_background_noises.sh"
echo "3. Or use generated synthetic realistic noises"
echo "4. Update Rust tests to load .wav files from $NOISE_DIR/"
echo ""
