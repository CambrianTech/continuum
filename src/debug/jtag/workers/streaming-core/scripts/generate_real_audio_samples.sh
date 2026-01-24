#!/bin/bash
# Generate real audio samples for VAD testing
# Uses macOS TTS (say) for real speech and ffmpeg for real noise profiles

set -e

OUTPUT_DIR="/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/workers/streaming-core/test_audio"
mkdir -p "$OUTPUT_DIR"

echo "Generating real audio samples for VAD testing..."

# Generate real speech using macOS TTS
echo "1. Generating real speech samples..."
say -o "$OUTPUT_DIR/speech_hello.aiff" "Hello, how are you doing today?"
say -o "$OUTPUT_DIR/speech_weather.aiff" "The weather is looking quite nice this afternoon."
say -o "$OUTPUT_DIR/speech_quick.aiff" "Quick brown fox jumps over the lazy dog."

# Convert to 16kHz mono WAV (required format for VAD)
for file in "$OUTPUT_DIR"/speech_*.aiff; do
    base=$(basename "$file" .aiff)
    ffmpeg -y -i "$file" -ar 16000 -ac 1 -sample_fmt s16 "$OUTPUT_DIR/${base}.wav"
    rm "$file"
done

# Generate real noise profiles using ffmpeg
echo "2. Generating real noise profiles..."

# Pink noise (1/f noise, common in nature)
ffmpeg -y -f lavfi -i "anoisesrc=d=5:c=pink:r=16000:a=0.3" -ar 16000 -ac 1 "$OUTPUT_DIR/noise_pink.wav"

# Brown noise (even lower frequency, like ocean waves)
ffmpeg -y -f lavfi -i "anoisesrc=d=5:c=brown:r=16000:a=0.3" -ar 16000 -ac 1 "$OUTPUT_DIR/noise_brown.wav"

# White noise (all frequencies equal)
ffmpeg -y -f lavfi -i "anoisesrc=d=5:c=white:r=16000:a=0.2" -ar 16000 -ac 1 "$OUTPUT_DIR/noise_white.wav"

# Generate noisy speech (mix speech with real noise at different SNR levels)
echo "3. Mixing speech with real noise at different SNR levels..."

# SNR +10dB (speech 10dB louder than noise)
ffmpeg -y -i "$OUTPUT_DIR/speech_hello.wav" -i "$OUTPUT_DIR/noise_pink.wav" \
    -filter_complex "[0:a]volume=1.0[speech];[1:a]volume=0.316[noise];[speech][noise]amix=inputs=2:duration=shortest" \
    -ar 16000 -ac 1 "$OUTPUT_DIR/noisy_speech_snr10.wav"

# SNR 0dB (speech and noise equal)
ffmpeg -y -i "$OUTPUT_DIR/speech_hello.wav" -i "$OUTPUT_DIR/noise_pink.wav" \
    -filter_complex "[0:a]volume=1.0[speech];[1:a]volume=1.0[noise];[speech][noise]amix=inputs=2:duration=shortest" \
    -ar 16000 -ac 1 "$OUTPUT_DIR/noisy_speech_snr0.wav"

# SNR -5dB (noise 5dB louder than speech)
ffmpeg -y -i "$OUTPUT_DIR/speech_hello.wav" -i "$OUTPUT_DIR/noise_pink.wav" \
    -filter_complex "[0:a]volume=1.0[speech];[1:a]volume=1.778[noise];[speech][noise]amix=inputs=2:duration=shortest" \
    -ar 16000 -ac 1 "$OUTPUT_DIR/noisy_speech_snr-5.wav"

echo "4. Generating short phoneme samples (plosives, fricatives)..."

# Use TTS for words with specific phonemes
say -o "$OUTPUT_DIR/phoneme_plosive.aiff" "Pop! Tap! Back! Pick!"
say -o "$OUTPUT_DIR/phoneme_fricative.aiff" "Sssss. Shhhh. Fffff."

# Convert phonemes to WAV
for file in "$OUTPUT_DIR"/phoneme_*.aiff; do
    base=$(basename "$file" .aiff)
    ffmpeg -y -i "$file" -ar 16000 -ac 1 -sample_fmt s16 "$OUTPUT_DIR/${base}.wav"
    rm "$file"
done

echo ""
echo "✅ Generated real audio samples:"
echo ""
ls -lh "$OUTPUT_DIR"/*.wav | awk '{print "  " $9 " (" $5 ")"}'
echo ""
echo "📊 Use these samples for realistic VAD testing"
