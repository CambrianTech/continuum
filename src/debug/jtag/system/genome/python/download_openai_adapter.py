#!/usr/bin/env python3
"""
Download OpenAI Fine-Tuned Adapter
===================================

Downloads a fine-tuned model from OpenAI and converts it to PEFT-compatible format.

Usage:
    python download_openai_adapter.py \
        --model-id "ft:gpt-4o-mini-2024-07-18:personal::CcKeiPN2" \
        --output-dir "./adapters/openai-wine-expertise"
"""

import argparse
import os
import sys
from pathlib import Path

from openai import OpenAI


def download_openai_adapter(model_id: str, output_dir: str, api_key: str | None = None):
    """
    Download fine-tuned adapter from OpenAI

    Args:
        model_id: OpenAI fine-tuned model ID (e.g., "ft:gpt-4o-mini-...")
        output_dir: Where to save the adapter files
        api_key: OpenAI API key (or None to use OPENAI_API_KEY env var)
    """
    print(f"📥 Downloading OpenAI adapter: {model_id}")
    print(f"   Output: {output_dir}")
    print()

    # Initialize OpenAI client
    client = OpenAI(api_key=api_key)

    # Create output directory
    os.makedirs(output_dir, exist_ok=True)

    # Get fine-tuning job details
    print("🔍 Fetching fine-tuning job details...")

    # Note: OpenAI doesn't provide direct adapter weight downloads yet
    # The fine-tuned model is accessed via API inference only
    # For now, we'll save metadata and use the model via API

    # Try to get the fine-tuning job that created this model
    try:
        jobs = client.fine_tuning.jobs.list(limit=100)
        matching_job = None

        for job in jobs.data:
            if job.fine_tuned_model == model_id:
                matching_job = job
                break

        if matching_job:
            print(f"✅ Found matching job: {matching_job.id}")
            print(f"   Status: {matching_job.status}")
            print(f"   Base model: {matching_job.model}")
            print(f"   Created at: {matching_job.created_at}")

            # Save metadata
            metadata = {
                "provider": "openai",
                "model_id": model_id,
                "job_id": matching_job.id,
                "base_model": matching_job.model,
                "status": matching_job.status,
                "created_at": matching_job.created_at,
                "trained_tokens": getattr(matching_job, 'trained_tokens', None),

                # Important note
                "note": "OpenAI fine-tuned models are accessed via API only. "
                        "Adapter weights are not downloadable. "
                        "Use this model_id for inference via OpenAI API.",

                # For PEFT composition
                "usage": {
                    "inference": "Use via OpenAI API with model_id",
                    "composition": "Cannot compose with local PEFT (API-only model)",
                    "alternative": "Train with Fireworks/Together for downloadable adapters"
                }
            }

            metadata_path = os.path.join(output_dir, "adapter_metadata.json")
            import json
            with open(metadata_path, 'w') as f:
                json.dump(metadata, indent=2, fp=f)

            print(f"\n✅ Metadata saved: {metadata_path}")
            print()
            print("⚠️  IMPORTANT:")
            print("   OpenAI does NOT provide downloadable adapter weights.")
            print("   This model can only be used via OpenAI API inference.")
            print("   For local PEFT composition, use Fireworks or Together adapters.")
            print()

            return metadata
        else:
            print(f"⚠️  Could not find job for model {model_id}")
            print("   Saving minimal metadata...")

            metadata = {
                "provider": "openai",
                "model_id": model_id,
                "note": "Job details not found. Model accessible via API."
            }

            metadata_path = os.path.join(output_dir, "adapter_metadata.json")
            import json
            with open(metadata_path, 'w') as f:
                json.dump(metadata, indent=2, fp=f)

            return metadata

    except Exception as e:
        print(f"❌ Error fetching job details: {e}")
        raise


def main():
    parser = argparse.ArgumentParser(description="Download OpenAI fine-tuned adapter")
    parser.add_argument("--model-id", required=True, help="OpenAI fine-tuned model ID")
    parser.add_argument("--output-dir", required=True, help="Output directory for adapter")
    parser.add_argument("--api-key", help="OpenAI API key (default: OPENAI_API_KEY env var)")

    args = parser.parse_args()

    try:
        metadata = download_openai_adapter(
            args.model_id,
            args.output_dir,
            args.api_key
        )

        print("=" * 80)
        print("DOWNLOAD COMPLETE")
        print("=" * 80)
        print()
        print(f"Model ID: {metadata['model_id']}")
        print(f"Provider: {metadata['provider']}")
        print(f"Output: {args.output_dir}")
        print()

        if 'usage' in metadata:
            print("NEXT STEPS:")
            print(f"  • Inference: {metadata['usage']['inference']}")
            print(f"  • Composition: {metadata['usage']['composition']}")
            print(f"  • Alternative: {metadata['usage']['alternative']}")

    except Exception as e:
        print(f"\n❌ DOWNLOAD FAILED: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main()
