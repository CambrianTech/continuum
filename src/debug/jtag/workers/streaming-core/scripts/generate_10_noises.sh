#!/bin/bash
# Generate 10 different realistic background noises for VAD testing

set -e

NOISE_DIR="test_audio/background_noise"
mkdir -p "$NOISE_DIR"

echo "Generating 10 realistic background noise samples..."
echo ""

# 1. White Noise (full spectrum, like TV static)
echo "1/10: White Noise (TV static)..."
ffmpeg -f lavfi -i "anoisesrc=duration=5:color=white:sample_rate=16000:amplitude=0.15" \
    -ar 16000 -ac 1 "$NOISE_DIR/01_white_noise.wav" -y 2>/dev/null
echo "  ✓ Generated: 01_white_noise.wav"

# 2. Pink Noise (1/f, more natural - rain-like)
echo "2/10: Pink Noise (rain, natural ambiance)..."
ffmpeg -f lavfi -i "anoisesrc=duration=5:color=pink:sample_rate=16000:amplitude=0.15" \
    -ar 16000 -ac 1 "$NOISE_DIR/02_pink_noise.wav" -y 2>/dev/null
echo "  ✓ Generated: 02_pink_noise.wav"

# 3. Brown Noise (1/f², low rumble - traffic, ocean)
echo "3/10: Brown Noise (traffic rumble, ocean)..."
ffmpeg -f lavfi -i "anoisesrc=duration=5:color=brown:sample_rate=16000:amplitude=0.15" \
    -ar 16000 -ac 1 "$NOISE_DIR/03_brown_noise.wav" -y 2>/dev/null
echo "  ✓ Generated: 03_brown_noise.wav"

# 4. HVAC / Air Conditioning (60Hz hum + broadband)
echo "4/10: HVAC / Air Conditioning..."
ffmpeg -f lavfi -i "sine=frequency=60:sample_rate=16000:duration=5" \
    -f lavfi -i "anoisesrc=duration=5:color=pink:sample_rate=16000:amplitude=0.05" \
    -filter_complex "[0:a][1:a]amix=inputs=2:duration=first:weights=0.3 0.7" \
    -ar 16000 -ac 1 "$NOISE_DIR/04_hvac_hum.wav" -y 2>/dev/null
echo "  ✓ Generated: 04_hvac_hum.wav"

# 5. Computer Fan / Ventilation (higher frequency hum)
echo "5/10: Computer Fan..."
ffmpeg -f lavfi -i "sine=frequency=120:sample_rate=16000:duration=5" \
    -f lavfi -i "anoisesrc=duration=5:color=white:sample_rate=16000:amplitude=0.08" \
    -filter_complex "[0:a][1:a]amix=inputs=2:duration=first:weights=0.2 0.8" \
    -ar 16000 -ac 1 "$NOISE_DIR/05_fan_noise.wav" -y 2>/dev/null
echo "  ✓ Generated: 05_fan_noise.wav"

# 6. Fluorescent Light Buzz (120Hz electrical hum)
echo "6/10: Fluorescent Light Buzz..."
ffmpeg -f lavfi -i "sine=frequency=120:sample_rate=16000:duration=5" \
    -f lavfi -i "sine=frequency=240:sample_rate=16000:duration=5" \
    -filter_complex "[0:a][1:a]amix=inputs=2:duration=first:weights=0.6 0.4" \
    -ar 16000 -ac 1 "$NOISE_DIR/06_fluorescent_buzz.wav" -y 2>/dev/null
echo "  ✓ Generated: 06_fluorescent_buzz.wav"

# 7. Office Ambiance (mix of frequencies simulating distant conversations)
echo "7/10: Office Ambiance..."
ffmpeg -f lavfi -i "anoisesrc=duration=5:color=pink:sample_rate=16000:amplitude=0.1" \
    -f lavfi -i "sine=frequency=200:sample_rate=16000:duration=5" \
    -f lavfi -i "sine=frequency=400:sample_rate=16000:duration=5" \
    -filter_complex "[0:a][1:a][2:a]amix=inputs=3:duration=first:weights=0.7 0.15 0.15" \
    -ar 16000 -ac 1 "$NOISE_DIR/07_office_ambiance.wav" -y 2>/dev/null
echo "  ✓ Generated: 07_office_ambiance.wav"

# 8. Crowd Murmur (bandpass filtered pink noise, 300-3000Hz)
echo "8/10: Crowd Murmur..."
ffmpeg -f lavfi -i "anoisesrc=duration=5:color=pink:sample_rate=16000:amplitude=0.2" \
    -af "bandpass=f=1000:width_type=h:w=2000" \
    -ar 16000 -ac 1 "$NOISE_DIR/08_crowd_murmur.wav" -y 2>/dev/null
echo "  ✓ Generated: 08_crowd_murmur.wav"

# 9. Traffic / Road Noise (low frequency brown + periodic louder events)
echo "9/10: Traffic / Road Noise..."
ffmpeg -f lavfi -i "anoisesrc=duration=5:color=brown:sample_rate=16000:amplitude=0.2" \
    -af "highpass=f=50,lowpass=f=500" \
    -ar 16000 -ac 1 "$NOISE_DIR/09_traffic_road.wav" -y 2>/dev/null
echo "  ✓ Generated: 09_traffic_road.wav"

# 10. Restaurant / Cafe (mid-frequency clatter + murmur)
echo "10/10: Restaurant / Cafe..."
ffmpeg -f lavfi -i "anoisesrc=duration=5:color=pink:sample_rate=16000:amplitude=0.15" \
    -f lavfi -i "anoisesrc=duration=5:color=white:sample_rate=16000:amplitude=0.05" \
    -filter_complex "[0:a][1:a]amix=inputs=2:duration=first:weights=0.7 0.3,bandpass=f=800:width_type=h:w=2000" \
    -ar 16000 -ac 1 "$NOISE_DIR/10_restaurant_cafe.wav" -y 2>/dev/null
echo "  ✓ Generated: 10_restaurant_cafe.wav"

echo ""
echo "✅ Successfully generated 10 background noise samples!"
echo ""
echo "Files created in: $NOISE_DIR/"
ls -lh "$NOISE_DIR"/*.wav | awk '{print "  " $9 " (" $5 ")"}'
echo ""
echo "All files are 16kHz mono WAV format, ready for VAD testing."
