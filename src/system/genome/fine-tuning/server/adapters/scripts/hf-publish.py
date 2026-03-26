"""
Publish a LoRA adapter to HuggingFace Hub

Reads the adapter manifest, generates a Continuum model card with training
metadata, and pushes to HuggingFace with standardized continuum:* tags.

Usage:
    python hf-publish.py --adapter-path /path/to/adapter --repo-id user/model-name [options]

Requirements:
    huggingface_hub (pip install huggingface_hub)
    Must be logged in: huggingface-cli login
"""

import argparse
import json
import os
import sys
from pathlib import Path

try:
    from huggingface_hub import HfApi, create_repo, upload_folder
    print("✅ huggingface_hub imported")
except ImportError:
    print("❌ Missing: pip install huggingface_hub")
    sys.exit(1)


def load_manifest(adapter_path: str) -> dict:
    manifest_path = os.path.join(adapter_path, 'manifest.json')
    if not os.path.exists(manifest_path):
        raise FileNotFoundError(f"No manifest.json in {adapter_path}")
    with open(manifest_path) as f:
        return json.load(f)


def normalize_tag_value(value: str) -> str:
    """Normalize tag value to lowercase kebab-case.
    Matches AdapterPublishSchema.ts normalizeTagValue() exactly.
    "Sprite Artist" → "sprite-artist", "sprite_artist" → "sprite-artist"
    """
    import re
    # camelCase → kebab-case
    value = re.sub(r'([a-z0-9])([A-Z])', r'\1-\2', value)
    # underscores and spaces → hyphens
    value = re.sub(r'[_\s]+', '-', value)
    # lowercase
    value = value.lower()
    # collapse multiple hyphens
    value = re.sub(r'-{2,}', '-', value)
    # trim
    return value.strip('-')


def normalize_base_model(model_id: str) -> str:
    """Strip org prefix, normalize. Matches AdapterPublishSchema.ts."""
    name = model_id.split('/')[-1] if '/' in model_id else model_id
    return normalize_tag_value(name)


# Schema version — must match CONTINUUM_TAG_SCHEMA_VERSION in AdapterPublishSchema.ts
SCHEMA_VERSION = 1


def build_tags(manifest: dict, project_type: str | None = None) -> list[str]:
    """Build standardized continuum:* tags from adapter manifest.
    Tag format defined in AdapterPublishSchema.ts — this MUST match exactly.
    """
    tags = ['peft', 'lora', 'continuum', f'continuum:schema={SCHEMA_VERSION}']

    # Base model — both HF native format and our normalized format
    if manifest.get('baseModel'):
        tags.append(f"base_model:{manifest['baseModel']}")
        tags.append(f"continuum:base={normalize_base_model(manifest['baseModel'])}")

    # Role (from traitType)
    if manifest.get('traitType'):
        tags.append(f"continuum:role={normalize_tag_value(manifest['traitType'])}")

    # Training performance — integers only
    meta = manifest.get('trainingMetadata', {})
    if meta.get('performance') is not None:
        tags.append(f"continuum:score={int(meta['performance'])}")
    if meta.get('epochs'):
        tags.append(f"continuum:epochs={int(meta['epochs'])}")

    # Persona
    if manifest.get('personaName'):
        tags.append(f"continuum:persona={normalize_tag_value(manifest['personaName'])}")

    # Project type
    if project_type:
        tags.append(f"continuum:project-type={normalize_tag_value(project_type)}")

    # Rank
    if manifest.get('rank'):
        tags.append(f"continuum:rank={int(manifest['rank'])}")

    return tags


def build_model_card(manifest: dict, tags: list[str],
                     academy_data: dict | None = None,
                     team_data: dict | None = None) -> str:
    """Generate a HuggingFace model card from adapter manifest + academy data."""
    name = manifest.get('name', 'Unnamed Adapter')
    trait = manifest.get('traitType', 'general')
    base = manifest.get('baseModel', 'unknown')
    persona = manifest.get('personaName', 'Unknown Persona')
    meta = manifest.get('trainingMetadata', {})

    sections = []

    # YAML frontmatter
    tag_yaml = '\n'.join(f'- {t}' for t in tags)
    sections.append(f"""---
tags:
{tag_yaml}
library_name: peft
base_model: {base}
---""")

    # Title
    sections.append(f"# {name}\n")

    # Trained by Continuum
    sections.append("## Trained by [Continuum](https://github.com/CambrianTech/continuum)\n")
    sections.append(f"This LoRA adapter was trained by **{persona}** (role: {trait}).")

    if team_data:
        team_size = team_data.get('memberCount', '?')
        project = team_data.get('projectDescription', '')
        sections.append(f"Part of a {team_size}-person team project: *{project}*\n")

    # Training results
    sections.append("\n## Training Results\n")
    if meta.get('performance') is not None:
        sections.append(f"- **Score:** {meta['performance']}/100")
    if meta.get('phenotypeScore') is not None:
        sections.append(f"- **Phenotype score:** {meta['phenotypeScore']}/100")
    if meta.get('phenotypeImprovement') is not None:
        sections.append(f"- **Improvement from training:** +{meta['phenotypeImprovement']} points")
    if meta.get('epochs'):
        sections.append(f"- **Epochs:** {meta['epochs']}")
    if meta.get('loss') is not None:
        sections.append(f"- **Final loss:** {meta['loss']:.4f}")
    if manifest.get('rank'):
        sections.append(f"- **LoRA rank:** {manifest['rank']}")
    sections.append(f"- **Base model:** `{base}`")

    # Academy session data (exam Q&A, before/after)
    if academy_data:
        if academy_data.get('examSample'):
            sample = academy_data['examSample']
            sections.append("\n## Example Output\n")
            sections.append(f"**Question:** {sample.get('question', 'N/A')}")
            if sample.get('beforeAnswer'):
                sections.append(f"\n**Before training:** {sample['beforeAnswer']}")
            if sample.get('afterAnswer'):
                sections.append(f"\n**After training:** {sample['afterAnswer']}")

    # Quick Start — reproducible procedure for verification
    sections.append("\n## Quick Start\n")

    # Detect available formats from manifest
    gguf_files = manifest.get('ggufFiles', [])
    has_safetensors = manifest.get('hasSafetensors', True)
    repo_id_str = manifest.get('_repoId', '<repo-id>')

    # llama.cpp (recommended for GGUF)
    if gguf_files:
        sections.append("### llama.cpp (recommended)\n")
        sections.append("```bash")
        # Use smallest GGUF as default example
        default_gguf = gguf_files[0] if gguf_files else f"{repo_id_str.split('/')[-1]}-Q4_K_M.gguf"
        sections.append(f"# Download")
        sections.append(f'huggingface-cli download {repo_id_str} {default_gguf} --local-dir .')
        sections.append(f"")
        sections.append(f"# Run server")
        sections.append(f'./llama-server -m {default_gguf} -c 4096 -ngl 99')
        sections.append(f"")
        sections.append(f"# Or interactive chat")
        sections.append(f'./llama-cli -m {default_gguf} -c 4096 -ngl 99 --chat-template chatml -cnv')
        sections.append("```\n")

        # GGUF file table
        sections.append("### Available Quantizations\n")
        sections.append("| File | Size | Use Case |")
        sections.append("|------|------|----------|")
        for gf in gguf_files:
            size = gf.get('size', '?')
            use_case = gf.get('useCase', '')
            sections.append(f"| `{gf['name']}` | {size} | {use_case} |")
        sections.append("")

    # Python / transformers
    if has_safetensors:
        sections.append("### Python (transformers)\n")
        sections.append("```python")
        sections.append("from transformers import AutoModelForCausalLM, AutoTokenizer")
        sections.append("import torch")
        sections.append("")
        sections.append(f'model = AutoModelForCausalLM.from_pretrained("{repo_id_str}",')
        sections.append('    torch_dtype=torch.bfloat16, device_map="auto", trust_remote_code=True)')
        sections.append(f'tokenizer = AutoTokenizer.from_pretrained("{repo_id_str}", trust_remote_code=True)')
        sections.append("```\n")

    # LoRA adapter usage (if this is a LoRA, not a full model)
    if manifest.get('traitType') and not manifest.get('compaction'):
        sections.append("### As LoRA adapter\n")
        sections.append("```python")
        sections.append("from peft import PeftModel, AutoModelForCausalLM")
        sections.append(f'base = AutoModelForCausalLM.from_pretrained("{base}")')
        sections.append(f'model = PeftModel.from_pretrained(base, "{repo_id_str}")')
        sections.append("```\n")

    # continuum usage
    sections.append("### In continuum\n")
    sections.append("```bash")
    sections.append(f'./jtag adapter/adopt --adapterId="{repo_id_str}"')
    sections.append("```\n")

    # Verification section — how to confirm the model works
    verification = manifest.get('qualityVerification', {})
    if verification:
        sections.append("## Verification\n")
        sections.append("To verify this model produces correct output:\n")
        for test_name, result in verification.items():
            sections.append(f"- **{test_name}**: {result}")
        sections.append("")

    # About
    sections.append("## Part of continuum\n")
    sections.append("[continuum](https://github.com/CambrianTech/continuum) is an open-source AI ecosystem")
    sections.append("where personas live, work, learn, and evolve on your hardware. Zero API keys required. AGPL-3.0.")
    sections.append("")
    sections.append("Built on the research foundations of [Synthetic Citizens](https://github.com/CambrianTech/continuum/blob/main/docs/papers/SYNTHETIC-CITIZENS.md).")
    sections.append("\n[Get started →](https://github.com/CambrianTech/continuum)")

    return '\n'.join(sections)


def main():
    parser = argparse.ArgumentParser(description="Publish LoRA adapter to HuggingFace")
    parser.add_argument("--adapter-path", required=True, help="Path to adapter directory")
    parser.add_argument("--repo-id", required=True, help="HuggingFace repo ID (user/model)")
    parser.add_argument("--project-type", default=None, help="Project type tag")
    parser.add_argument("--academy-data", default=None, help="JSON file with academy session data")
    parser.add_argument("--team-data", default=None, help="JSON file with team project data")
    parser.add_argument("--private", action="store_true", help="Create private repo")
    parser.add_argument("--update", action="store_true", help="Update existing repo")
    args = parser.parse_args()

    print(f"📦 Publishing adapter to HuggingFace: {args.repo_id}")

    # Load manifest
    manifest = load_manifest(args.adapter_path)
    print(f"   Adapter: {manifest.get('name', 'unknown')} ({manifest.get('traitType', 'unknown')})")
    print(f"   Base model: {manifest.get('baseModel', 'unknown')}")

    # Load optional academy/team data
    academy_data = None
    if args.academy_data and os.path.exists(args.academy_data):
        with open(args.academy_data) as f:
            academy_data = json.load(f)

    team_data = None
    if args.team_data and os.path.exists(args.team_data):
        with open(args.team_data) as f:
            team_data = json.load(f)

    # Build tags
    tags = build_tags(manifest, args.project_type)
    print(f"   Tags: {tags}")

    # Generate model card
    model_card = build_model_card(manifest, tags, academy_data, team_data)

    # Write model card to adapter directory
    card_path = os.path.join(args.adapter_path, 'README.md')
    with open(card_path, 'w') as f:
        f.write(model_card)
    print(f"   Model card written to {card_path}")

    # Create or get repo
    api = HfApi()
    try:
        if not args.update:
            create_repo(args.repo_id, private=args.private, exist_ok=True)
            print(f"   Repo created/verified: {args.repo_id}")
        else:
            print(f"   Updating existing repo: {args.repo_id}")
    except Exception as e:
        print(f"❌ Failed to create repo: {e}")
        sys.exit(1)

    # Upload adapter directory
    try:
        url = upload_folder(
            folder_path=args.adapter_path,
            repo_id=args.repo_id,
            ignore_patterns=["checkpoint-*", "optimizer.pt", "scheduler.pt",
                           "training_args.bin", "rng_state.pth", "runs/*",
                           "*.log", "__pycache__"],
            commit_message=f"Publish {manifest.get('name', 'adapter')} via Continuum",
        )
        print(f"✅ Published: https://huggingface.co/{args.repo_id}")
    except Exception as e:
        print(f"❌ Upload failed: {e}")
        sys.exit(1)

    # Output structured result for sentinel parsing
    result = {
        "success": True,
        "repoUrl": f"https://huggingface.co/{args.repo_id}",
        "tags": tags,
        "modelCardGenerated": True,
    }
    print(f"PUBLISH_RESULT: {json.dumps(result)}")


if __name__ == "__main__":
    main()
