#!/bin/bash
# Test TTS Audio Generation
# Captures synthesized audio and saves to WAV for playback verification

echo "🎙️ Testing TTS Audio Generation"
echo "================================"
echo ""

TEXT="Hello world, this is a test of AI voice synthesis"
echo "📝 Text: \"$TEXT\""
echo ""

# Call voice/synthesize and capture result
echo "⏳ Synthesizing speech..."
RESULT=$(./jtag voice/synthesize --text="$TEXT" --adapter=piper 2>&1)
HANDLE=$(echo "$RESULT" | jq -r '.handle')

echo "✅ Command executed, handle: $HANDLE"
echo ""

# Wait for synthesis to complete
echo "⏳ Waiting for audio events (5 seconds)..."
sleep 5

# Check server logs for the audio event
echo "📊 Checking logs for audio data..."
LOG_FILE=".continuum/jtag/logs/system/npm-start.log"

# Extract base64 audio from logs (looking for the voice:audio event)
# This is hacky but works for testing
AUDIO_BASE64=$(tail -200 "$LOG_FILE" | grep "voice:audio:$HANDLE" -A 20 | grep -o '"audio":"[^"]*"' | head -1 | cut -d'"' -f4)

if [ -z "$AUDIO_BASE64" ]; then
  echo "❌ No audio data found in logs"
  echo ""
  echo "Recent log entries:"
  tail -50 "$LOG_FILE" | grep -E "(synthesize|audio|$HANDLE)" | tail -20
  exit 1
fi

AUDIO_LEN=${#AUDIO_BASE64}
echo "✅ Found audio data: $AUDIO_LEN chars base64"
echo ""

# Decode base64 to binary
echo "🔧 Decoding base64 audio..."
echo "$AUDIO_BASE64" | base64 -d > /tmp/tts-test-raw.pcm

PCM_SIZE=$(wc -c < /tmp/tts-test-raw.pcm | tr -d ' ')
echo "✅ Decoded PCM: $PCM_SIZE bytes"
echo ""

# Convert PCM to WAV using sox (if available) or manual WAV header
if command -v sox &> /dev/null; then
  echo "🎵 Converting to WAV using sox..."
  sox -r 16000 -e signed-integer -b 16 -c 1 /tmp/tts-test-raw.pcm /tmp/tts-test.wav
else
  echo "⚠️  sox not available, creating WAV manually..."
  # Manual WAV header creation would go here
  # For now, just use ffmpeg if available
  if command -v ffmpeg &> /dev/null; then
    echo "🎵 Converting to WAV using ffmpeg..."
    ffmpeg -f s16le -ar 16000 -ac 1 -i /tmp/tts-test-raw.pcm /tmp/tts-test.wav -y 2>&1 | grep -E "(Duration|Stream|size)"
  else
    echo "❌ Neither sox nor ffmpeg available, cannot create WAV"
    echo "   Raw PCM saved to: /tmp/tts-test-raw.pcm"
    echo "   Format: 16-bit signed PCM, 16kHz, mono"
    exit 1
  fi
fi

WAV_SIZE=$(wc -c < /tmp/tts-test.wav | tr -d ' ')
DURATION=$(echo "scale=2; $PCM_SIZE / 2 / 16000" | bc)

echo ""
echo "💾 Saved to: /tmp/tts-test.wav"
echo "📏 Duration: ${DURATION}s"
echo "🎵 Sample rate: 16000Hz"
echo "📦 File size: $WAV_SIZE bytes"
echo ""

echo "🎧 To play:"
echo "   afplay /tmp/tts-test.wav"
echo "   OR open /tmp/tts-test.wav"
echo ""

# Try to play automatically if on macOS
if command -v afplay &> /dev/null; then
  echo "🔊 Playing audio..."
  afplay /tmp/tts-test.wav
  echo "✅ Playback complete!"
else
  echo "ℹ️  afplay not available (not on macOS?)"
fi

echo ""
echo "✅ Test complete!"
