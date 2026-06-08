"""
Expert Pruning — Step 2 of Plasticity Compaction for MoE models.

Takes the activation profile from Step 1 and physically removes
low-activation expert tensors from the safetensors files. Produces
a new, smaller model directory that can be quantized and published.

Usage:
    python3 prune_experts.py --keep 167 [--profile /tmp/runtime_activation_profile.json]
"""

import argparse
import json
import os
import shutil
import time
from pathlib import Path

import torch
from safetensors.torch import load_file, save_file

MODEL_PATH = os.path.expanduser("~/.continuum/models/qwen3.5-35b-a3b-opus")
PROFILE_PATH = "/tmp/runtime_activation_profile.json"


def load_profile(profile_path: str, keep_count: int) -> set:
    """Load activation profile and return set of expert IDs to KEEP."""
    with open(profile_path) as f:
        data = json.load(f)

    ranked = data["ranked"]
    keep_ids = set()
    for i, entry in enumerate(ranked):
        if i >= keep_count:
            break
        keep_ids.add(entry["id"])

    total = data["total"]
    kept_activations = sum(e["count"] for e in ranked[:keep_count])
    coverage = kept_activations / total * 100 if total > 0 else 0

    print(f"Keeping {len(keep_ids)}/{len(ranked)} experts ({coverage:.1f}% routing coverage)")
    print(f"Pruning {len(ranked) - len(keep_ids)} experts")

    return keep_ids


def identify_expert_tensors(model_path: str) -> dict:
    """Scan safetensors to identify which tensors belong to which experts."""
    shard_files = sorted(Path(model_path).glob("*.safetensors"))
    expert_tensors = {}  # expert_id -> [(shard_path, tensor_name)]
    non_expert_tensors = {}  # tensor_name -> shard_path

    for shard_path in shard_files:
        tensors = load_file(str(shard_path), device="cpu")
        for name in tensors.keys():
            # MoE expert tensors have patterns like:
            # model.layers.N.mlp.experts.M.gate_proj.weight
            # model.layers.N.mlp.experts.M.up_proj.weight
            # model.layers.N.mlp.experts.M.down_proj.weight
            if ".experts." in name:
                parts = name.split(".")
                # Find the expert index
                expert_idx = None
                for i, part in enumerate(parts):
                    if part == "experts" and i + 1 < len(parts):
                        try:
                            expert_idx = int(parts[i + 1])
                        except ValueError:
                            pass
                        break

                if expert_idx is not None:
                    if expert_idx not in expert_tensors:
                        expert_tensors[expert_idx] = []
                    expert_tensors[expert_idx].append((str(shard_path), name))
            else:
                non_expert_tensors[name] = str(shard_path)

        # Free memory
        del tensors

    return expert_tensors, non_expert_tensors


def prune_and_save(
    model_path: str,
    output_path: str,
    keep_ids: set,
    expert_tensors: dict,
    non_expert_tensors: dict,
    num_experts_original: int = 256,
):
    """Create pruned model by copying kept expert tensors + all non-expert tensors."""
    os.makedirs(output_path, exist_ok=True)

    # Copy non-safetensor files (config, tokenizer, etc.)
    for f in Path(model_path).iterdir():
        if f.suffix != ".safetensors" and f.name != ".git":
            dest = Path(output_path) / f.name
            if f.is_file():
                shutil.copy2(str(f), str(dest))

    # Count what we're keeping vs pruning
    kept_expert_count = len([eid for eid in expert_tensors if eid in keep_ids])
    pruned_expert_count = len([eid for eid in expert_tensors if eid not in keep_ids])
    print(f"Expert tensors: keeping {kept_expert_count}, pruning {pruned_expert_count}")
    print(f"Non-expert tensors: {len(non_expert_tensors)} (all kept)")

    # Build the new tensor map — remap expert indices to be contiguous
    # Old: experts 0,1,3,5,64,... -> New: experts 0,1,2,3,4,...
    keep_sorted = sorted(keep_ids)
    old_to_new = {old_id: new_id for new_id, old_id in enumerate(keep_sorted)}

    # Collect all tensors for the pruned model
    all_tensors = {}

    # Load non-expert tensors
    print("Loading non-expert tensors...", flush=True)
    loaded_shards = {}
    for name, shard_path in non_expert_tensors.items():
        if shard_path not in loaded_shards:
            loaded_shards[shard_path] = load_file(shard_path, device="cpu")
        all_tensors[name] = loaded_shards[shard_path][name]

    # Load kept expert tensors with remapped indices
    print("Loading kept expert tensors...", flush=True)
    for old_id in keep_sorted:
        new_id = old_to_new[old_id]
        if old_id not in expert_tensors:
            continue
        for shard_path, name in expert_tensors[old_id]:
            if shard_path not in loaded_shards:
                loaded_shards[shard_path] = load_file(shard_path, device="cpu")

            # Remap the expert index in the tensor name
            new_name = name.replace(f".experts.{old_id}.", f".experts.{new_id}.")
            all_tensors[new_name] = loaded_shards[shard_path][name]

    # Free loaded shards
    del loaded_shards

    # Save as single safetensor file (or sharded if too large)
    print(f"Saving {len(all_tensors)} tensors to {output_path}...", flush=True)

    # Estimate total size
    total_bytes = sum(t.numel() * t.element_size() for t in all_tensors.values())
    total_gb = total_bytes / 1e9
    print(f"Total tensor data: {total_gb:.1f}GB")

    # Save in shards of ~5GB each
    shard_size = 5 * 1024 * 1024 * 1024  # 5GB
    current_shard = {}
    current_size = 0
    shard_idx = 1
    shard_files = []

    for name, tensor in sorted(all_tensors.items()):
        tensor_size = tensor.numel() * tensor.element_size()
        if current_size + tensor_size > shard_size and current_shard:
            shard_name = f"model-{shard_idx:05d}-of-TOTAL.safetensors"
            save_file(current_shard, os.path.join(output_path, shard_name))
            shard_files.append(shard_name)
            print(f"  Saved shard {shard_idx}: {len(current_shard)} tensors, {current_size/1e9:.1f}GB", flush=True)
            shard_idx += 1
            current_shard = {}
            current_size = 0

        current_shard[name] = tensor
        current_size += tensor_size

    # Save final shard
    if current_shard:
        shard_name = f"model-{shard_idx:05d}-of-TOTAL.safetensors"
        save_file(current_shard, os.path.join(output_path, shard_name))
        shard_files.append(shard_name)
        print(f"  Saved shard {shard_idx}: {len(current_shard)} tensors, {current_size/1e9:.1f}GB", flush=True)

    # Fix shard names with correct total
    total_shards = len(shard_files)
    for old_name in shard_files:
        new_name = old_name.replace("TOTAL", f"{total_shards:05d}")
        os.rename(
            os.path.join(output_path, old_name),
            os.path.join(output_path, new_name),
        )

    # Update config.json with new expert count
    config_path = os.path.join(output_path, "config.json")
    with open(config_path) as f:
        config = json.load(f)

    config["text_config"]["num_experts"] = len(keep_ids)
    config["text_config"]["_original_num_experts"] = num_experts_original
    config["text_config"]["_pruned_experts"] = num_experts_original - len(keep_ids)
    config["text_config"]["_pruning_coverage_pct"] = round(
        sum(1 for _ in keep_ids) / num_experts_original * 100, 1
    )

    with open(config_path, "w") as f:
        json.dump(config, f, indent=4)

    # Create model index
    index = {
        "metadata": {
            "total_size": total_bytes,
            "compaction": {
                "method": "expert_pruning",
                "original_experts": num_experts_original,
                "kept_experts": len(keep_ids),
                "pruned_experts": num_experts_original - len(keep_ids),
            },
        },
        "weight_map": {},
    }
    # Build weight map
    for shard_name_old in shard_files:
        shard_name_new = shard_name_old.replace("TOTAL", f"{total_shards:05d}")
        shard_path = os.path.join(output_path, shard_name_new)
        shard_tensors = load_file(shard_path, device="cpu")
        for tensor_name in shard_tensors.keys():
            index["weight_map"][tensor_name] = shard_name_new
        del shard_tensors

    with open(os.path.join(output_path, "model.safetensors.index.json"), "w") as f:
        json.dump(index, f, indent=2)

    print(f"\nPruned model saved to: {output_path}")
    print(f"  Experts: {len(keep_ids)}/{num_experts_original}")
    print(f"  Size: {total_gb:.1f}GB BF16")
    print(f"  Shards: {total_shards}")


def main():
    parser = argparse.ArgumentParser(description="Prune MoE experts based on activation profile")
    parser.add_argument("--model", default=MODEL_PATH, help="Path to original model")
    parser.add_argument("--output", default=None, help="Output directory for pruned model")
    parser.add_argument("--profile", default=PROFILE_PATH, help="Activation profile JSON")
    parser.add_argument("--keep", type=int, default=167, help="Number of experts to keep (default: 167 = 80%% coverage)")
    args = parser.parse_args()

    if args.output is None:
        args.output = args.model.rstrip("/") + f"-pruned-{args.keep}experts"

    print(f"=== MoE Expert Pruning ===")
    print(f"Model: {args.model}")
    print(f"Output: {args.output}")
    print(f"Keep: {args.keep}/256 experts")
    print(f"Profile: {args.profile}")
    print()

    keep_ids = load_profile(args.profile, args.keep)
    expert_tensors, non_expert_tensors = identify_expert_tensors(args.model)

    print(f"\nFound {len(expert_tensors)} expert groups, {len(non_expert_tensors)} non-expert tensors")

    prune_and_save(
        args.model, args.output, keep_ids,
        expert_tensors, non_expert_tensors,
        num_experts_original=256,
    )


if __name__ == "__main__":
    main()
