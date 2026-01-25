#!/usr/bin/env python3
"""
Download Kokoro TTS model from HuggingFace

Kokoro is the #1 rated TTS on TTS Arena (80.9% win rate).
This script downloads the ONNX model for CPU inference.

Model: hexgrad/Kokoro-82M
License: Apache 2.0
Size: ~82MB
"""

import os
import sys
import urllib.request
from pathlib import Path

# Model configuration
HUGGINGFACE_REPO = "hexgrad/Kokoro-82M"
MODEL_FILE = "kokoro-v0_19.onnx"
VOICES_FILE = "voices.json"

# Determine models directory (relative to this script)
SCRIPT_DIR = Path(__file__).parent
WORKERS_DIR = SCRIPT_DIR.parent
MODELS_DIR = WORKERS_DIR / "models" / "kokoro"

# HuggingFace URLs
BASE_URL = f"https://huggingface.co/{HUGGINGFACE_REPO}/resolve/main"
MODEL_URL = f"{BASE_URL}/{MODEL_FILE}"
VOICES_URL = f"{BASE_URL}/{VOICES_FILE}"


def download_file(url: str, dest_path: Path) -> bool:
    """Download file with progress bar"""
    print(f"Downloading {url}...")
    print(f"  → {dest_path}")

    try:
        def progress_hook(count, block_size, total_size):
            if total_size > 0:
                percent = int(count * block_size * 100 / total_size)
                mb_downloaded = count * block_size / (1024 * 1024)
                mb_total = total_size / (1024 * 1024)
                print(
                    f"\r  Progress: {percent}% ({mb_downloaded:.1f}/{mb_total:.1f} MB)",
                    end="",
                )

        urllib.request.urlretrieve(url, dest_path, reporthook=progress_hook)
        print()  # New line after progress
        return True
    except Exception as e:
        print(f"\n  ✗ Failed: {e}", file=sys.stderr)
        return False


def main():
    print("=" * 60)
    print("Kokoro TTS Model Download")
    print("=" * 60)
    print()
    print(f"Model: {HUGGINGFACE_REPO}")
    print(f"License: Apache 2.0")
    print(f"Destination: {MODELS_DIR}")
    print()

    # Create models directory
    MODELS_DIR.mkdir(parents=True, exist_ok=True)
    print(f"✓ Created directory: {MODELS_DIR}")
    print()

    # Download ONNX model
    model_path = MODELS_DIR / MODEL_FILE
    if model_path.exists():
        print(f"✓ Model already exists: {model_path}")
        print(f"  Size: {model_path.stat().st_size / (1024*1024):.1f} MB")
        print()
    else:
        if not download_file(MODEL_URL, model_path):
            print("Failed to download model", file=sys.stderr)
            sys.exit(1)
        print(f"✓ Downloaded: {model_path}")
        print(f"  Size: {model_path.stat().st_size / (1024*1024):.1f} MB")
        print()

    # Download voices.json
    voices_path = MODELS_DIR / VOICES_FILE
    if voices_path.exists():
        print(f"✓ Voices already exist: {voices_path}")
    else:
        if not download_file(VOICES_URL, voices_path):
            print("Warning: Failed to download voices.json (non-critical)", file=sys.stderr)
        else:
            print(f"✓ Downloaded: {voices_path}")
    print()

    print("=" * 60)
    print("✅ Kokoro TTS model ready!")
    print("=" * 60)
    print()
    print("Next steps:")
    print("  1. Restart streaming-core worker:")
    print(f"     cd {WORKERS_DIR}")
    print("     cargo run --release --bin streaming-core")
    print()
    print("  2. Test TTS:")
    print('     ./jtag voice/synthesize --text="Hello world"')
    print()


if __name__ == "__main__":
    main()
