#!/usr/bin/env python3
"""
Download Piper TTS model from Hugging Face

Piper is a fast, local neural TTS system with high-quality voices.
This script downloads the en_US-libritts_r-medium model.

Model: rhasspy/piper-voices
License: MIT / CC BY-NC-SA 4.0 (voice specific)
Size: ~63MB (model) + ~0.2MB (config)
"""

import os
import sys
import urllib.request
import json
from pathlib import Path

# Model configuration
VOICE = "en_US-libritts_r-medium"
MODEL_FILE = f"{VOICE}.onnx"
CONFIG_FILE = f"{VOICE}.onnx.json"

# Determine models directory
SCRIPT_DIR = Path(__file__).parent
WORKERS_DIR = SCRIPT_DIR.parent
MODELS_DIR = WORKERS_DIR / "models" / "piper"

# Hugging Face URLs (using CDN)
BASE_URL = "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/libritts_r/medium"
MODEL_URL = f"{BASE_URL}/{MODEL_FILE}"
CONFIG_URL = f"{BASE_URL}/{CONFIG_FILE}"


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
    print("Piper TTS Model Download")
    print("=" * 60)
    print()
    print(f"Voice: {VOICE}")
    print(f"Quality: Medium (good balance of speed/quality)")
    print(f"License: MIT")
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

    # Download config JSON
    config_path = MODELS_DIR / CONFIG_FILE
    if config_path.exists():
        print(f"✓ Config already exists: {config_path}")
    else:
        if not download_file(CONFIG_URL, config_path):
            print("Warning: Failed to download config (non-critical)", file=sys.stderr)
        else:
            print(f"✓ Downloaded: {config_path}")
            # Validate JSON
            try:
                with open(config_path) as f:
                    config = json.load(f)
                    print(f"  Sample rate: {config.get('audio', {}).get('sample_rate', 'unknown')} Hz")
            except:
                pass
    print()

    print("=" * 60)
    print("✅ Piper TTS model ready!")
    print("=" * 60)
    print()
    print("Next steps:")
    print("  1. Restart streaming-core worker:")
    print(f"     cd {WORKERS_DIR}")
    print("     cargo run --release --bin streaming-core")
    print()
    print("  2. Test TTS:")
    print('     ./jtag voice/synthesize --adapter=piper --text="Hello world"')
    print()


if __name__ == "__main__":
    main()
