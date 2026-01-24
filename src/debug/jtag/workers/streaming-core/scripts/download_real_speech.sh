#!/bin/bash
# Download REAL human speech samples for proper VAD testing

set -e

SPEECH_DIR="test_audio/real_speech"
mkdir -p "$SPEECH_DIR"

echo "Downloading REAL human speech samples..."
echo ""

# LibriSpeech test-clean samples (public domain audiobooks)
echo "1. LibriSpeech test samples (high quality, read speech)..."
# Small subset for testing
curl -L "https://www.openslr.org/resources/12/test-clean.tar.gz" -o /tmp/librispeech-test.tar.gz 2>/dev/null || echo "Download failed"

# Common Voice samples (Creative Commons, conversational)
echo "2. Mozilla Common Voice samples..."
echo "   (Requires manual download from https://commonvoice.mozilla.org/)"

# Alternative: VCTK Corpus (public, multiple speakers)
echo "3. VCTK Corpus samples..."
curl -L "https://datashare.ed.ac.uk/bitstream/handle/10283/3443/VCTK-Corpus-0.92.zip" -o /tmp/vctk.zip 2>/dev/null || echo "Download too large"

echo ""
echo "Manual download recommended:"
echo "  LibriSpeech: https://www.openslr.org/12"
echo "  Common Voice: https://commonvoice.mozilla.org/"
echo "  VCTK: https://datashare.ed.ac.uk/handle/10283/3443"
