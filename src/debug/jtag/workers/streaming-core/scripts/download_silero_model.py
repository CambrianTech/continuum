#!/usr/bin/env python3
"""
Download Silero VAD model using Python

This script downloads the Silero VAD ONNX model from Hugging Face or GitHub,
handling Git LFS properly.
"""

import os
import sys
import urllib.request
from pathlib import Path

# Model URLs (try in order)
URLS = [
    # Hugging Face (most reliable)
    "https://huggingface.co/snakers4/silero-vad/resolve/main/files/silero_vad.onnx",
    # Direct LFS URL (requires computing SHA)
    "https://media.githubusercontent.com/media/snakers4/silero-vad/master/files/silero_vad.onnx",
]

MODEL_SIZE_EXPECTED = 1840400  # ~1.8MB
MODEL_NAME = "silero_vad.onnx"


def download_model(output_dir: Path) -> bool:
    """Download Silero VAD model to output directory"""
    output_path = output_dir / MODEL_NAME

    print(f"Downloading Silero VAD model to: {output_path}")
    print("This may take a moment...")

    for i, url in enumerate(URLS, 1):
        print(f"\nAttempt {i}/{len(URLS)}: {url}")

        try:
            # Download with progress
            def report(count, block_size, total_size):
                if total_size > 0:
                    percent = count * block_size * 100 / total_size
                    sys.stdout.write(f"\r  Progress: {percent:.1f}%")
                    sys.stdout.flush()

            urllib.request.urlretrieve(url, output_path, reporthook=report)
            print()  # New line after progress

            # Verify file size
            file_size = output_path.stat().st_size
            print(f"  Downloaded: {file_size / 1024:.1f} KB")

            if file_size < 100000:  # Less than 100KB = likely HTML error page
                print(f"  ⚠️  File too small, likely not the model")
                output_path.unlink()
                continue

            # Check if it's a real ONNX file
            with open(output_path, 'rb') as f:
                magic = f.read(4)
                if magic != b'onnx':  # ONNX files don't start with this, let me check
                    # Actually ONNX files are protobuf, check for HTML instead
                    f.seek(0)
                    first_bytes = f.read(100)
                    if b'<html' in first_bytes.lower() or b'<!doctype' in first_bytes.lower():
                        print(f"  ⚠️  Downloaded HTML, not ONNX model")
                        output_path.unlink()
                        continue

            print(f"  ✅ Model downloaded successfully!")
            return True

        except Exception as e:
            print(f"  ❌ Failed: {e}")
            if output_path.exists():
                output_path.unlink()
            continue

    return False


def main():
    # Get script directory
    script_dir = Path(__file__).parent
    models_dir = script_dir.parent / "models"

    # Create models directory
    models_dir.mkdir(exist_ok=True)

    print("═" * 60)
    print("  Silero VAD Model Downloader")
    print("═" * 60)
    print()

    # Check if already downloaded
    model_path = models_dir / MODEL_NAME
    if model_path.exists():
        file_size = model_path.stat().st_size
        if file_size > 1000000:  # > 1MB
            print(f"✅ Model already exists: {model_path}")
            print(f"   Size: {file_size / 1024:.1f} KB")
            print()
            print("To re-download, delete the file first:")
            print(f"   rm {model_path}")
            return 0

    # Download
    success = download_model(models_dir)

    print()
    if success:
        print("✅ Setup complete!")
        print()
        print("Next steps:")
        print("  1. Run tests: cargo test --test vad_production -- --ignored")
        print("  2. See docs: cat docs/QUICK-START.md")
        return 0
    else:
        print("❌ All download attempts failed")
        print()
        print("Manual download:")
        print("  1. Clone repo with LFS: git lfs clone https://github.com/snakers4/silero-vad")
        print("  2. Copy file: cp silero-vad/files/silero_vad.onnx models/")
        print()
        print("Or use alternative:")
        print("  1. Install silero-vad: pip install silero-vad")
        print("  2. The model will be auto-downloaded on first use")
        return 1


if __name__ == "__main__":
    sys.exit(main())
