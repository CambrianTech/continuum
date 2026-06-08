"""
Static Gate Weight Analysis — Prune BEFORE loading.

Analyzes MoE router/gate weights directly from safetensors files
WITHOUT loading the full model. The gate weight magnitudes indicate
which experts the model prefers to route to. Low-magnitude gate
columns = rarely-selected experts = safe to prune.

This is step 0: decide what to prune. Step 1: physically remove
the pruned expert tensors. Step 2: load the smaller model for fine-tuning.
"""

import json
import os
import sys
import time
from collections import defaultdict
from pathlib import Path

import torch
from safetensors import safe_open

MODEL_PATH = os.path.expanduser("~/.continuum/models/qwen3.5-35b-a3b-opus")


def find_gate_tensors(model_path: str) -> dict[str, torch.Tensor]:
    """Scan all safetensor shards for gate/router weight tensors."""
    gate_tensors = {}
    shard_files = sorted(Path(model_path).glob("*.safetensors"))

    print(f"Scanning {len(shard_files)} safetensor shards...")

    for shard_path in shard_files:
        with safe_open(str(shard_path), framework="pt", device="cpu") as f:
            for key in f.keys():
                # MoE gate/router weights — these decide expert routing
                if "gate" in key.lower() or "router" in key.lower():
                    tensor = f.get_tensor(key)
                    gate_tensors[key] = tensor
                    print(f"  Found: {key} — shape {list(tensor.shape)}")

    return gate_tensors


def analyze_expert_importance(gate_tensors: dict[str, torch.Tensor], num_experts: int = 256):
    """
    Analyze gate weights to determine expert importance.

    For each gate weight matrix (hidden_size x num_experts), the column norms
    indicate how strongly the router directs tokens to each expert. Higher norm
    = more likely to be selected = more important.
    """
    # Aggregate importance across all layers
    expert_importance = defaultdict(float)
    expert_counts = defaultdict(int)
    layer_analysis = {}

    for name, tensor in sorted(gate_tensors.items()):
        # Gate weight shape is typically (num_experts, hidden_size) or (hidden_size, num_experts)
        if tensor.dim() != 2:
            print(f"  Skipping {name}: dim={tensor.dim()}, shape={list(tensor.shape)}")
            continue

        # Determine orientation
        if tensor.shape[0] == num_experts:
            # (num_experts, hidden_size) — each row is an expert's gate vector
            expert_norms = tensor.float().norm(dim=1)  # L2 norm per expert
        elif tensor.shape[1] == num_experts:
            # (hidden_size, num_experts) — each column is an expert's gate vector
            expert_norms = tensor.float().norm(dim=0)
        else:
            # Try to infer — check which dim matches or is close to num_experts
            print(f"  {name}: shape {list(tensor.shape)} doesn't match {num_experts} experts, analyzing anyway")
            # Use the dimension that's closer to num_experts
            dim0_diff = abs(tensor.shape[0] - num_experts)
            dim1_diff = abs(tensor.shape[1] - num_experts)
            if dim0_diff < dim1_diff:
                expert_norms = tensor.float().norm(dim=1)
                actual_experts = tensor.shape[0]
            else:
                expert_norms = tensor.float().norm(dim=0)
                actual_experts = tensor.shape[1]

        actual_experts = len(expert_norms)

        # Normalize to [0, 1] range
        if expert_norms.max() > 0:
            normalized = expert_norms / expert_norms.max()
        else:
            normalized = expert_norms

        # Extract layer index from name
        layer_name = name.rsplit(".", 1)[0] if "." in name else name

        layer_analysis[name] = {
            "shape": list(tensor.shape),
            "num_experts_detected": actual_experts,
            "mean_norm": float(expert_norms.mean()),
            "std_norm": float(expert_norms.std()),
            "min_norm": float(expert_norms.min()),
            "max_norm": float(expert_norms.max()),
            "low_importance_count": int((normalized < 0.1).sum()),  # < 10% of max
            "medium_importance_count": int(((normalized >= 0.1) & (normalized < 0.5)).sum()),
            "high_importance_count": int((normalized >= 0.5).sum()),
        }

        for i in range(actual_experts):
            expert_importance[i] += float(normalized[i])
            expert_counts[i] += 1

    return expert_importance, expert_counts, layer_analysis


def generate_pruning_plan(
    expert_importance: dict,
    expert_counts: dict,
    num_experts: int = 256,
    active_per_token: int = 8,
):
    """Generate a pruning plan based on expert importance scores."""

    # Average importance across layers
    avg_importance = {}
    for eid in range(num_experts):
        if expert_counts.get(eid, 0) > 0:
            avg_importance[eid] = expert_importance[eid] / expert_counts[eid]
        else:
            avg_importance[eid] = 0.0

    # Sort by importance (highest first)
    ranked = sorted(avg_importance.items(), key=lambda x: x[1], reverse=True)

    # Cumulative importance
    total_importance = sum(v for v in avg_importance.values())
    cumulative = 0
    thresholds = {}

    for i, (eid, imp) in enumerate(ranked):
        cumulative += imp
        pct = cumulative / total_importance * 100 if total_importance > 0 else 0
        for target in [80, 90, 95, 99]:
            if target not in thresholds and pct >= target:
                thresholds[target] = i + 1

    # Print results
    print(f"\n{'='*60}")
    print(f"EXPERT IMPORTANCE ANALYSIS")
    print(f"{'='*60}")
    print(f"Total experts: {num_experts}")
    print(f"Active per token: {active_per_token}")
    print(f"Gate tensors analyzed: {len(expert_counts)}")
    print(f"Total importance score: {total_importance:.2f}")

    print(f"\n--- Importance Distribution ---")
    zero_importance = sum(1 for v in avg_importance.values() if v < 0.01)
    low_importance = sum(1 for v in avg_importance.values() if 0.01 <= v < 0.2)
    mid_importance = sum(1 for v in avg_importance.values() if 0.2 <= v < 0.5)
    high_importance = sum(1 for v in avg_importance.values() if v >= 0.5)
    print(f"  Near-zero (<0.01):  {zero_importance} experts — SAFE TO PRUNE")
    print(f"  Low (0.01-0.2):     {low_importance} experts — likely prunable")
    print(f"  Medium (0.2-0.5):   {mid_importance} experts")
    print(f"  High (≥0.5):        {high_importance} experts — KEEP")

    print(f"\n--- Pruning Thresholds ---")
    for pct in [80, 90, 95, 99]:
        keep = thresholds.get(pct, num_experts)
        prune = num_experts - keep
        # Estimate size savings (expert params per layer)
        expert_params = 2 * 2048 * 512  # up + down projection per expert
        saved_bytes = prune * expert_params * 40 * 2  # 40 layers, BF16
        saved_gb = saved_bytes / 1e9
        print(f"  {pct}% importance coverage: keep {keep}, prune {prune} — save ~{saved_gb:.1f}GB BF16")

    print(f"\n--- Top 20 Most Important Experts ---")
    for i, (eid, imp) in enumerate(ranked[:20]):
        print(f"  #{i+1}: Expert {eid} — importance {imp:.4f}")

    print(f"\n--- Bottom 20 Least Important Experts ---")
    for eid, imp in ranked[-20:]:
        print(f"  Expert {eid} — importance {imp:.6f}")

    # Build pruning plan
    plan = {
        "model": "Qwen3.5-35B-A3B-Claude-4.6-Opus-Reasoning-Distilled",
        "analyzed_at": time.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "num_experts": num_experts,
        "active_per_token": active_per_token,
        "total_importance": total_importance,
        "importance_distribution": {
            "near_zero": zero_importance,
            "low": low_importance,
            "medium": mid_importance,
            "high": high_importance,
        },
        "thresholds": {str(k): v for k, v in thresholds.items()},
        "ranked_experts": [{"id": eid, "importance": round(imp, 6)} for eid, imp in ranked],
        "pruning_recommendations": {},
    }

    for target_keep in [32, 48, 64, 96]:
        keep_ids = [eid for eid, _ in ranked[:target_keep]]
        prune_ids = [eid for eid, _ in ranked[target_keep:]]
        kept_importance = sum(avg_importance[eid] for eid in keep_ids)
        coverage = kept_importance / total_importance * 100 if total_importance > 0 else 0

        expert_params = 2 * 2048 * 512
        original_gb = num_experts * expert_params * 40 * 2 / 1e9
        remaining_gb = target_keep * expert_params * 40 * 2 / 1e9
        saved_gb = original_gb - remaining_gb

        plan["pruning_recommendations"][f"keep_{target_keep}"] = {
            "keep_expert_ids": keep_ids,
            "prune_expert_ids": prune_ids,
            "importance_coverage_pct": round(coverage, 2),
            "experts_pruned": len(prune_ids),
            "expert_size_saved_gb": round(saved_gb, 1),
            "estimated_total_after_q4_gb": round((remaining_gb + 24.1) * 4 / 16, 1),
        }

    return plan


def main():
    print(f"=== Static Gate Weight Analysis ===")
    print(f"Model: {MODEL_PATH}")
    print()

    gate_tensors = find_gate_tensors(MODEL_PATH)

    if not gate_tensors:
        print("ERROR: No gate/router tensors found in model.")
        print("Listing all tensor names to help debug:")
        for shard in sorted(Path(MODEL_PATH).glob("*.safetensors")):
            with safe_open(str(shard), framework="pt", device="cpu") as f:
                for key in f.keys():
                    if "expert" in key.lower() or "moe" in key.lower() or "gate" in key.lower():
                        print(f"  {key}")
        sys.exit(1)

    with open(os.path.join(MODEL_PATH, "config.json")) as f:
        config = json.load(f)
    num_experts = config["text_config"]["num_experts"]
    active_per_tok = config["text_config"]["num_experts_per_tok"]

    importance, counts, layer_analysis = analyze_expert_importance(gate_tensors, num_experts)
    plan = generate_pruning_plan(importance, counts, num_experts, active_per_tok)

    # Add layer analysis
    plan["layer_analysis"] = {k: v for k, v in layer_analysis.items()}

    output_path = "/tmp/pruning_plan.json"
    with open(output_path, "w") as f:
        json.dump(plan, f, indent=2)
    print(f"\nPruning plan saved to: {output_path}")


if __name__ == "__main__":
    main()
