#!/usr/bin/env python3
"""
Compact safetensors model by pruning attention heads based on utilization data.

This is the Python fallback compactor for when the Rust IPC server isn't running.
It reads gate_gradients.json, computes which heads to prune, and physically
removes the corresponding rows/columns from attention weight tensors.

Usage:
    python3 compact-safetensors.py \
        --gradients path/to/gate_gradients.json \
        --model-dir path/to/model-directory \
        --output-dir path/to/output

The output directory will contain:
    - compacted_model.safetensors (single file, all shards merged)
    - head_topology.json (per-layer head counts and precision assignments)
    - analysis.json (detailed analysis of what was pruned/quantized)
"""

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Dict, List, Optional, Tuple

try:
    import safetensors
    from safetensors import safe_open
    from safetensors.torch import save_file
    import torch
except ImportError:
    print("ERROR: Required packages not installed.")
    print("  pip install safetensors torch")
    sys.exit(1)


# ── Precision tiers ───────────────────────────────────────────────────

TIERS = {
    "removed": (0.0, 0.1),
    "ternary": (0.1, 0.2),
    "q2": (0.2, 0.3),
    "q4": (0.3, 0.5),
    "q8": (0.5, 0.7),
    "bf16": (0.7, float("inf")),
}


def score_to_tier(score: float) -> str:
    for tier, (lo, hi) in TIERS.items():
        if lo <= score < hi:
            return tier
    return "bf16"


# ── Model config ─────────────────────────────────────────────────────

def read_model_config(model_dir: Path) -> dict:
    config_path = model_dir / "config.json"
    with open(config_path) as f:
        config = json.load(f)

    num_heads = config.get("num_attention_heads", config.get("n_head", 0))
    num_kv_heads = config.get("num_key_value_heads", config.get("n_head_kv", num_heads))
    hidden_size = config.get("hidden_size", 0)
    num_layers = config.get("num_hidden_layers", 0)
    head_dim = hidden_size // num_heads if num_heads > 0 else 0

    intermediate_size = config.get("intermediate_size", hidden_size * 7 // 2)
    vocab_size = config.get("vocab_size", 32000)

    return {
        "num_heads": num_heads,
        "num_kv_heads": num_kv_heads,
        "hidden_size": hidden_size,
        "num_layers": num_layers,
        "head_dim": head_dim,
        "intermediate_size": intermediate_size,
        "vocab_size": vocab_size,
        "gqa_ratio": num_heads // num_kv_heads if num_kv_heads > 0 else 1,
    }


# ── Topology computation ─────────────────────────────────────────────

def compute_topology(
    gradients: dict,
    model_config: dict,
    min_heads: int = 4,
    min_kv_heads: int = 2,
    target_size_gb: Optional[float] = None,
) -> dict:
    """Compute per-layer head retention decisions from utilization scores.

    When target_size_gb is set, uses budget-aware allocation: sorts all heads
    by utilization, greedily assigns the highest affordable precision tier
    within the byte budget. This is critical for models like 32B where fixed
    thresholds might not remove any heads (all scores > 0.4).
    """
    num_heads = model_config["num_heads"]
    num_kv_heads = model_config["num_kv_heads"]
    hidden_size = model_config["hidden_size"]
    head_dim = model_config["head_dim"]
    gqa_ratio = model_config["gqa_ratio"]
    num_layers = model_config["num_layers"]
    layers = gradients["layer_scores"]

    if target_size_gb is not None:
        return _compute_budget_aware_topology(
            gradients, model_config, min_heads, min_kv_heads, target_size_gb
        )

    return _compute_threshold_topology(
        gradients, model_config, min_heads, min_kv_heads
    )


def _estimate_non_attention_bytes(model_config: dict) -> int:
    """Estimate bytes for non-attention params (MLP, embeddings, norms).

    These are copied verbatim — only attention is compactable.
    """
    hidden = model_config["hidden_size"]
    num_layers = model_config["num_layers"]
    # Read from config.json if available
    intermediate = model_config.get("intermediate_size", hidden * 7 // 2)
    vocab = model_config.get("vocab_size", 32000)
    bf16 = 2

    embeddings = vocab * hidden * bf16
    mlp_per_layer = 3 * hidden * intermediate * bf16  # gate_proj, up_proj, down_proj
    norms_per_layer = 2 * hidden * bf16
    final_norm = hidden * bf16
    lm_head = vocab * hidden * bf16

    return embeddings + (mlp_per_layer + norms_per_layer) * num_layers + final_norm + lm_head


def _compute_budget_aware_topology(
    gradients: dict,
    model_config: dict,
    min_heads: int,
    min_kv_heads: int,
    target_size_gb: float,
) -> dict:
    """Budget-aware: fit model into target_size_gb by optimally allocating precision."""
    num_heads = model_config["num_heads"]
    num_kv_heads = model_config["num_kv_heads"]
    hidden_size = model_config["hidden_size"]
    head_dim = model_config["head_dim"]
    gqa_ratio = model_config["gqa_ratio"]
    num_layers = model_config["num_layers"]
    layers = gradients["layer_scores"]

    target_bytes = int(target_size_gb * 1_073_741_824)
    non_attn_bytes = _estimate_non_attention_bytes(model_config)

    # KV heads stay BF16 (quantizing shared KV is risky)
    params_per_kv_head = head_dim * hidden_size * 2  # K + V projections
    kv_bytes_total = num_kv_heads * num_layers * params_per_kv_head * 2  # BF16

    attention_budget = target_bytes - non_attn_bytes
    q_budget = max(0, attention_budget - kv_bytes_total)

    print(f"  Budget: target={target_size_gb:.1f}GB, non-attention={non_attn_bytes/1e9:.2f}GB, "
          f"KV={kv_bytes_total/1e9:.2f}GB, Q-head budget={q_budget/1e9:.2f}GB")

    # Collect all Q heads with scores
    all_heads = []
    for layer_idx, head_scores in enumerate(layers):
        for head_idx, score in enumerate(head_scores):
            all_heads.append((layer_idx, head_idx, score))

    # Sort by score descending — best heads get best precision
    all_heads.sort(key=lambda x: -x[2])

    # Precision tiers: bytes_per_param for Q+O projection parameters
    # Q head contributes: head_dim * hidden_size (Q proj) + hidden_size * head_dim (O proj)
    params_per_q_head = head_dim * hidden_size * 2
    tier_bytes = [
        ("bf16", 2.0),
        ("q8", 1.0),
        ("q4", 0.5),
        ("q2", 0.25),
        ("ternary", 0.2),
    ]

    # Initialize all as removed
    assignments = [["removed"] * num_heads for _ in range(num_layers)]
    used_bytes = 0

    for layer_idx, head_idx, score in all_heads:
        for tier_name, bpp in tier_bytes:
            cost = int(params_per_q_head * bpp)
            if used_bytes + cost <= q_budget:
                assignments[layer_idx][head_idx] = tier_name
                used_bytes += cost
                break
        # If no tier fits, stays "removed"

    # Apply GQA constraints and min-heads floors
    return _build_topology_from_assignments(
        gradients, model_config, assignments, min_heads, min_kv_heads
    )


def _compute_threshold_topology(
    gradients: dict,
    model_config: dict,
    min_heads: int,
    min_kv_heads: int,
) -> dict:
    """Fixed-threshold topology (original algorithm)."""
    num_heads = model_config["num_heads"]
    num_kv_heads = model_config["num_kv_heads"]
    gqa_ratio = model_config["gqa_ratio"]
    layers = gradients["layer_scores"]

    assignments = []
    for layer_idx, head_scores in enumerate(layers):
        layer_tiers = [score_to_tier(s) for s in head_scores]
        assignments.append(layer_tiers)

    return _build_topology_from_assignments(
        gradients, model_config, assignments, min_heads, min_kv_heads
    )


def _build_topology_from_assignments(
    gradients: dict,
    model_config: dict,
    assignments: List[List[str]],
    min_heads: int,
    min_kv_heads: int,
) -> dict:
    """Build final topology from per-head precision assignments, applying GQA + min-heads."""
    num_heads = model_config["num_heads"]
    num_kv_heads = model_config["num_kv_heads"]
    head_dim = model_config["head_dim"]
    gqa_ratio = model_config["gqa_ratio"]
    layers = gradients["layer_scores"]

    topology_layers = []
    total_original = 0
    total_retained = 0
    precision_profile = {"removed": 0, "ternary": 0, "q2": 0, "q4": 0, "q8": 0, "bf16": 0}

    for layer_idx, head_scores in enumerate(layers):
        tiers = list(assignments[layer_idx])

        # GQA constraint: KV head survives if ANY Q head in its group is alive
        kv_alive = []
        for kv_idx in range(num_kv_heads):
            q_start = kv_idx * gqa_ratio
            q_end = min((kv_idx + 1) * gqa_ratio, num_heads)
            alive = any(tiers[q] != "removed" for q in range(q_start, q_end))
            kv_alive.append(alive)

        # Promote dead Q heads in alive KV groups to ternary
        for kv_idx in range(num_kv_heads):
            q_start = kv_idx * gqa_ratio
            q_end = min((kv_idx + 1) * gqa_ratio, num_heads)
            if kv_alive[kv_idx]:
                for q in range(q_start, q_end):
                    if tiers[q] == "removed":
                        tiers[q] = "ternary"

        # Enforce minimum heads
        alive_count = sum(1 for t in tiers if t != "removed")
        if alive_count < min_heads:
            removed_indices = [
                (head_scores[i], i) for i in range(num_heads) if tiers[i] == "removed"
            ]
            removed_indices.sort(reverse=True)
            for _, idx in removed_indices[:min_heads - alive_count]:
                tiers[idx] = "ternary"

        # Build retained indices
        retained_q = [i for i in range(num_heads) if tiers[i] != "removed"]
        retained_kv = [i for i in range(num_kv_heads) if kv_alive[i]]

        # Enforce minimum KV heads
        if len(retained_kv) < min_kv_heads:
            for kv_idx in range(num_kv_heads):
                if kv_idx not in retained_kv:
                    retained_kv.append(kv_idx)
                    retained_kv.sort()
                    q_start = kv_idx * gqa_ratio
                    q_end = min((kv_idx + 1) * gqa_ratio, num_heads)
                    for q in range(q_start, q_end):
                        if tiers[q] == "removed":
                            tiers[q] = "ternary"
                            if q not in retained_q:
                                retained_q.append(q)
                                retained_q.sort()
                if len(retained_kv) >= min_kv_heads:
                    break

        head_precisions = [tiers[i] for i in retained_q]
        head_scores_retained = [head_scores[i] for i in retained_q]

        for t in tiers:
            precision_profile[t] += 1

        total_original += num_heads
        total_retained += len(retained_q)

        topology_layers.append({
            "layerIndex": layer_idx,
            "numHeads": len(retained_q),
            "numKvHeads": len(retained_kv),
            "retainedHeadIndices": retained_q,
            "retainedKvHeadIndices": retained_kv,
            "headPrecisions": head_precisions,
            "headScores": head_scores_retained,
        })

    parameter_reduction = 1.0 - (total_retained / total_original) if total_original > 0 else 0.0

    return {
        "baseModel": gradients["model_name"],
        "layers": topology_layers,
        "originalNumHeads": num_heads,
        "originalNumKvHeads": num_kv_heads,
        "headDim": head_dim,
        "parameterReduction": parameter_reduction,
        "precisionProfile": precision_profile,
        "createdAt": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }


# ── Tensor compaction ─────────────────────────────────────────────────

def parse_attention_tensor_name(name: str) -> Optional[Tuple[int, str]]:
    """Parse 'model.layers.N.self_attn.{q,k,v,o}_proj.{weight,bias}' pattern."""
    parts = name.split(".")
    for i in range(len(parts) - 4):
        if parts[i] == "layers" and parts[i + 2] == "self_attn":
            try:
                layer_idx = int(parts[i + 1])
            except ValueError:
                continue
            proj = parts[i + 3]
            if proj in ("q_proj", "k_proj", "v_proj", "o_proj"):
                suffix = parts[i + 4] if i + 4 < len(parts) else "weight"
                return (layer_idx, f"{proj}.{suffix}")
    return None


def compact_tensor(
    name: str,
    tensor: torch.Tensor,
    topology: dict,
) -> torch.Tensor:
    """Compact a single tensor by removing pruned head rows/columns."""
    parsed = parse_attention_tensor_name(name)
    if parsed is None:
        return tensor  # Non-attention tensor: copy verbatim

    layer_idx, proj_type = parsed
    if layer_idx >= len(topology["layers"]):
        return tensor

    layer = topology["layers"][layer_idx]
    head_dim = topology["headDim"]
    retained_q = layer["retainedHeadIndices"]
    retained_kv = layer["retainedKvHeadIndices"]

    if proj_type == "q_proj.weight":
        # Shape: [num_heads * head_dim, hidden_size] — slice rows
        indices = []
        for h in retained_q:
            indices.extend(range(h * head_dim, (h + 1) * head_dim))
        return tensor[indices, :]

    elif proj_type in ("k_proj.weight", "v_proj.weight"):
        # Shape: [num_kv_heads * head_dim, hidden_size] — slice rows by KV heads
        indices = []
        for h in retained_kv:
            indices.extend(range(h * head_dim, (h + 1) * head_dim))
        return tensor[indices, :]

    elif proj_type == "o_proj.weight":
        # Shape: [hidden_size, num_heads * head_dim] — slice columns
        indices = []
        for h in retained_q:
            indices.extend(range(h * head_dim, (h + 1) * head_dim))
        return tensor[:, indices]

    elif proj_type == "q_proj.bias":
        indices = []
        for h in retained_q:
            indices.extend(range(h * head_dim, (h + 1) * head_dim))
        return tensor[indices]

    elif proj_type in ("k_proj.bias", "v_proj.bias"):
        indices = []
        for h in retained_kv:
            indices.extend(range(h * head_dim, (h + 1) * head_dim))
        return tensor[indices]

    elif proj_type == "o_proj.bias":
        return tensor  # O bias is per hidden_size, not per head

    return tensor


# ── Main pipeline ─────────────────────────────────────────────────────

def discover_shards(model_dir: Path) -> List[Path]:
    """Find safetensors files in a model directory."""
    single = model_dir / "model.safetensors"
    if single.exists():
        return [single]

    shards = sorted(
        p for p in model_dir.iterdir()
        if p.name.startswith("model-") and p.name.endswith(".safetensors") and "-of-" in p.name
    )

    if not shards:
        raise FileNotFoundError(
            f"No safetensors files found in {model_dir}. "
            "Expected model.safetensors or model-NNNNN-of-NNNNN.safetensors"
        )
    return shards


def run_compaction(
    gradients_path: Path,
    model_dir: Path,
    output_dir: Path,
    target_size_gb: Optional[float] = None,
):
    """Full compaction pipeline."""
    # 1. Load gradients
    print(f"Loading gate gradients from {gradients_path}")
    with open(gradients_path) as f:
        gradients = json.load(f)

    # 2. Read model config
    print(f"Reading model config from {model_dir}")
    model_config = read_model_config(model_dir)
    print(f"  {model_config['num_layers']} layers, {model_config['num_heads']} heads "
          f"({model_config['num_kv_heads']} KV), head_dim={model_config['head_dim']}")
    print(f"  hidden_size={model_config['hidden_size']}, "
          f"intermediate_size={model_config['intermediate_size']}, "
          f"vocab_size={model_config['vocab_size']}")

    # 3. Compute topology
    if target_size_gb:
        print(f"Computing budget-aware topology for {target_size_gb:.1f}GB target...")
    else:
        print("Computing fixed-threshold topology...")
    topology = compute_topology(gradients, model_config, target_size_gb=target_size_gb)
    print(f"  Parameter reduction: {topology['parameterReduction'] * 100:.1f}%")
    pp = topology["precisionProfile"]
    print(f"  Profile: removed={pp['removed']} ternary={pp['ternary']} "
          f"q2={pp['q2']} q4={pp['q4']} q8={pp['q8']} bf16={pp['bf16']}")

    # 4. Discover shards
    shards = discover_shards(model_dir)
    print(f"Found {len(shards)} safetensor shard(s)")

    # 5. Compact each shard's tensors
    # Process and accumulate in memory, deleting original shards after reading
    # to stay within disk quota (original + compacted > volume size).
    all_tensors = {}
    total_original_bytes = 0

    for shard_idx, shard_path in enumerate(shards):
        print(f"  Processing shard {shard_idx + 1}/{len(shards)}: {shard_path.name}")
        total_original_bytes += shard_path.stat().st_size

        with safe_open(str(shard_path), framework="pt", device="cpu") as f:
            for name in f.keys():
                tensor = f.get_tensor(name)
                compacted = compact_tensor(name, tensor, topology)
                all_tensors[name] = compacted

                # Show shape changes for attention tensors
                if tensor.shape != compacted.shape:
                    print(f"    {name}: {list(tensor.shape)} → {list(compacted.shape)}")

        # Delete original shard after processing to free disk space.
        # This is critical for large models where original + compacted > disk quota.
        shard_path.unlink()
        print(f"    (deleted {shard_path.name} to free disk)")

    # 6. Save compacted model
    output_dir.mkdir(parents=True, exist_ok=True)
    output_path = output_dir / "compacted_model.safetensors"
    print(f"\nSaving compacted model to {output_path}")
    save_file(all_tensors, str(output_path))

    compacted_size = output_path.stat().st_size

    # 7. Save topology
    topology_path = output_dir / "head_topology.json"
    with open(topology_path, "w") as f:
        json.dump(topology, f, indent=2)

    # 8. Save analysis
    analysis = {
        "topology": topology,
        "originalSizeBytes": total_original_bytes,
        "compactedSizeBytes": compacted_size,
        "reductionPercent": (1 - compacted_size / total_original_bytes) * 100 if total_original_bytes > 0 else 0,
    }
    analysis_path = output_dir / "analysis.json"
    with open(analysis_path, "w") as f:
        json.dump(analysis, f, indent=2)

    # 9. Summary
    print()
    print(f"Original:  {total_original_bytes / 1e9:.2f} GB")
    print(f"Compacted: {compacted_size / 1e9:.2f} GB")
    print(f"Reduction: {analysis['reductionPercent']:.1f}%")
    print()
    print(f"Files written:")
    print(f"  {output_path}")
    print(f"  {topology_path}")
    print(f"  {analysis_path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Compact safetensors model by pruning attention heads")
    parser.add_argument("--gradients", required=True, help="Path to gate_gradients.json")
    parser.add_argument("--model-dir", required=True, help="Path to model directory")
    parser.add_argument("--output-dir", required=True, help="Path to output directory")
    parser.add_argument("--target-size-gb", type=float, default=None,
                        help="Target model size in GB. Uses budget-aware allocation to fit.")
    args = parser.parse_args()

    run_compaction(
        gradients_path=Path(args.gradients),
        model_dir=Path(args.model_dir),
        output_dir=Path(args.output_dir),
        target_size_gb=args.target_size_gb,
    )
