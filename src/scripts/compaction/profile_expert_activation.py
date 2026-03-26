"""
Expert Activation Profiler — Step 1 of Plasticity Compaction for MoE models.

Runs inference on domain-representative prompts and records which experts
activate per layer. Output: per-expert activation frequency matrix that
drives the pruning decision.

Usage:
    python3 profile_expert_activation.py [--prompts prompts.jsonl] [--output activation_profile.json]

Requires: torch, transformers, safetensors
"""

import argparse
import json
import os
import sys
import time
from collections import defaultdict
from pathlib import Path

import torch

MODEL_PATH = os.path.expanduser("~/.continuum/models/qwen3.5-35b-a3b-opus")

# Domain-representative prompts for Continuum's use case:
# coding, tool calling, UI design, reasoning, conversation
DEFAULT_PROMPTS = [
    # Coding
    "Write a TypeScript function that implements a rate limiter using the token bucket algorithm. Include proper types and error handling.",
    "Fix this Rust code that has a lifetime error: fn longest(x: &str, y: &str) -> &str { if x.len() > y.len() { x } else { y } }",
    "Refactor this Python function to use list comprehension instead of a for loop with append.",
    "Write a SQL query that finds the top 5 customers by total order value, including customers with no orders (show $0).",
    "Implement a WebSocket reconnection handler in TypeScript with exponential backoff and jitter.",

    # Tool calling / XML format
    "I need to read the file at src/main.ts, search for all functions that call the database, and then edit the connection pooling configuration.",
    "Take a screenshot of the current page, then navigate to the settings tab and change the theme to dark mode.",
    "Search the codebase for all uses of deprecated API v1 endpoints and list the files that need updating.",
    "Create a new file called UserService.ts with a class that handles CRUD operations for users, using our existing ORM patterns.",
    "Run the test suite, find any failing tests, read the test files to understand what they expect, and fix the implementation.",

    # UI Design
    "The chat input box needs rounded corners with a 12px border radius and a subtle 1px border in the theme's secondary color. The send button should have a hover animation.",
    "Looking at this layout, the sidebar is too wide on mobile. We need a responsive breakpoint at 768px that collapses it into a hamburger menu.",
    "Design a dashboard card component that shows a metric value, a sparkline chart, and a trend indicator. Use CSS grid for the layout.",
    "The avatar thumbnails in the member list are pixelated. They need to use object-fit: cover and have a circular clip-path with a status indicator dot.",

    # Reasoning
    "Compare the tradeoffs between using a B-tree index vs a hash index for our chat_messages table, considering our access patterns: range queries on timestamp, point lookups on id, and prefix matches on room_id.",
    "A user reports that the system is slow after running for 3 days. Memory usage is at 8GB. Walk through a systematic debugging approach.",
    "Should we use WebSockets or Server-Sent Events for our real-time chat updates? Consider: bidirectional needs, reconnection, scaling, browser support.",
    "Explain why our MoE model with 256 experts but only 8 active per token might be more efficient than a dense 35B model for domain-specific tasks.",

    # Conversation / personality
    "Hey team, I just pushed a fix for the login bug. Can someone review PR #42? It touches the auth middleware and session handling.",
    "I'm feeling overwhelmed by the number of open issues. Can we prioritize and maybe close some that are no longer relevant?",
    "What's the most elegant way to handle error boundaries in our widget system? I want errors in one widget to not crash the whole page.",
    "Good morning everyone! What are we working on today? I'd love to help with whatever needs the most attention.",
]


def profile_experts(model_path: str, prompts: list[str], device: str = "cuda"):
    """
    Load model and profile expert activation across prompts.

    Hooks into the MoE router to capture gate decisions per layer per token.
    Returns activation counts per expert per layer.
    """
    from transformers import AutoModelForCausalLM, AutoTokenizer, AutoProcessor

    print(f"Loading tokenizer from {model_path}...")
    try:
        processor = AutoProcessor.from_pretrained(model_path, trust_remote_code=True)
        tokenizer = processor.tokenizer if hasattr(processor, 'tokenizer') else processor
    except Exception:
        tokenizer = AutoTokenizer.from_pretrained(model_path, trust_remote_code=True)

    print(f"Loading model (this will use ~32GB+ RAM for BF16)...")
    print(f"Device: {device}, VRAM: {torch.cuda.get_device_properties(0).total_memory / 1e9:.1f}GB" if device == "cuda" else f"Device: {device}")

    # Load 4-bit quantized for profiling — we just need gate routing decisions,
    # not full precision inference. 67GB BF16 won't fit in 32GB VRAM.
    # Allow CPU offload for overflow layers.
    from transformers import BitsAndBytesConfig
    quantization_config = BitsAndBytesConfig(
        load_in_4bit=True,
        bnb_4bit_compute_dtype=torch.bfloat16,
        bnb_4bit_quant_type="nf4",
        llm_int8_enable_fp32_cpu_offload=True,
    )
    # Explicit max_memory: use most of GPU + plenty of CPU RAM for overflow
    max_memory = {0: "28GiB", "cpu": "48GiB"}
    model = AutoModelForCausalLM.from_pretrained(
        model_path,
        quantization_config=quantization_config,
        device_map="auto",
        max_memory=max_memory,
        trust_remote_code=True,
    )
    model.eval()

    print(f"Model loaded. Parameters: {sum(p.numel() for p in model.parameters()):,}")

    # Find MoE router modules and install hooks
    activation_counts = defaultdict(lambda: defaultdict(int))  # layer -> expert_id -> count
    total_tokens_per_layer = defaultdict(int)

    hooks = []

    def make_hook(layer_idx):
        def hook_fn(module, input, output):
            # Router output contains the expert selection
            # For Qwen MoE, the router produces logits over experts
            # The top-k selection determines which experts activate
            if isinstance(output, tuple):
                router_logits = output[1] if len(output) > 1 else None
            else:
                router_logits = None

            # Try to capture from the module's internal state
            # Different MoE implementations store this differently
            if hasattr(module, 'gate'):
                gate = module.gate
                if hasattr(gate, 'weight'):
                    # We need the actual routing decisions
                    # These are made during forward pass
                    pass

            # For most MoE implementations, we hook the gate/router directly
            # and capture the argmax/topk of the gate logits
            if router_logits is not None:
                # router_logits shape: (batch, seq_len, num_experts)
                if router_logits.dim() == 3:
                    topk_experts = router_logits.topk(8, dim=-1).indices  # top 8
                    for expert_id in topk_experts.flatten().tolist():
                        activation_counts[layer_idx][expert_id] += 1
                    total_tokens_per_layer[layer_idx] += router_logits.shape[0] * router_logits.shape[1]
        return hook_fn

    # Install hooks on MoE layers
    hooked = 0
    for name, module in model.named_modules():
        # Look for MoE gate/router modules
        if any(keyword in name.lower() for keyword in ['moe', 'gate', 'router', 'experts']):
            if hasattr(module, 'forward'):
                layer_idx = name  # Use full name as layer identifier
                hooks.append(module.register_forward_hook(make_hook(layer_idx)))
                hooked += 1

    print(f"Installed {hooked} hooks on MoE modules")

    if hooked == 0:
        print("WARNING: No MoE modules found for hooking. Trying alternative approach...")
        # Print model structure to help debug
        for name, module in model.named_modules():
            if 'expert' in name.lower() or 'moe' in name.lower() or 'gate' in name.lower():
                print(f"  Found: {name} ({type(module).__name__})")

    # Run inference on all prompts
    print(f"\nProfiling {len(prompts)} prompts...")
    for i, prompt in enumerate(prompts):
        tokens = tokenizer(prompt, return_tensors="pt", truncation=True, max_length=512)
        tokens = {k: v.to(model.device) for k, v in tokens.items()}

        with torch.no_grad():
            start = time.time()
            outputs = model.generate(
                **tokens,
                max_new_tokens=128,
                do_sample=False,
                temperature=1.0,
            )
            elapsed = time.time() - start

        gen_tokens = outputs.shape[1] - tokens['input_ids'].shape[1]
        print(f"  [{i+1}/{len(prompts)}] {gen_tokens} tokens in {elapsed:.1f}s — {prompt[:60]}...")

    # Clean up hooks
    for h in hooks:
        h.remove()

    return dict(activation_counts), dict(total_tokens_per_layer)


def analyze_and_save(activation_counts, total_tokens, output_path: str):
    """Analyze activation patterns and save profile."""

    profile = {
        "model": "Qwen3.5-35B-A3B-Claude-4.6-Opus-Reasoning-Distilled",
        "num_experts": 256,
        "active_per_token": 8,
        "profiled_at": time.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "layers": {},
    }

    all_expert_totals = defaultdict(int)

    for layer_name, expert_counts in sorted(activation_counts.items()):
        total = total_tokens.get(layer_name, 1)
        layer_profile = {}

        for expert_id, count in sorted(expert_counts.items()):
            freq = count / total if total > 0 else 0
            layer_profile[str(expert_id)] = {
                "count": count,
                "frequency": round(freq, 6),
            }
            all_expert_totals[expert_id] += count

        # Find unused experts in this layer
        used_experts = set(expert_counts.keys())
        unused = [i for i in range(256) if i not in used_experts]

        profile["layers"][layer_name] = {
            "total_tokens": total,
            "experts_used": len(used_experts),
            "experts_unused": len(unused),
            "expert_frequencies": layer_profile,
        }

    # Global summary
    total_activations = sum(all_expert_totals.values())
    expert_usage = sorted(all_expert_totals.items(), key=lambda x: x[1], reverse=True)

    profile["summary"] = {
        "total_activations": total_activations,
        "experts_with_activations": len(all_expert_totals),
        "experts_never_activated": 256 - len(all_expert_totals),
        "top_20_experts": [{"id": eid, "count": c, "pct": round(c/total_activations*100, 2)} for eid, c in expert_usage[:20]],
        "bottom_20_experts": [{"id": eid, "count": c, "pct": round(c/total_activations*100, 2)} for eid, c in expert_usage[-20:]],
    }

    # Pruning recommendations
    cumulative = 0
    keep_thresholds = {}
    for i, (eid, count) in enumerate(expert_usage):
        cumulative += count
        pct = cumulative / total_activations * 100
        if 90 not in keep_thresholds and pct >= 90:
            keep_thresholds[90] = i + 1
        if 95 not in keep_thresholds and pct >= 95:
            keep_thresholds[95] = i + 1
        if 99 not in keep_thresholds and pct >= 99:
            keep_thresholds[99] = i + 1

    profile["pruning_recommendations"] = {
        "keep_for_90pct_coverage": keep_thresholds.get(90, 256),
        "keep_for_95pct_coverage": keep_thresholds.get(95, 256),
        "keep_for_99pct_coverage": keep_thresholds.get(99, 256),
    }

    with open(output_path, 'w') as f:
        json.dump(profile, f, indent=2)

    print(f"\n=== Expert Activation Profile ===")
    print(f"Layers profiled: {len(activation_counts)}")
    print(f"Experts with activations: {len(all_expert_totals)}/256")
    print(f"Experts never activated: {256 - len(all_expert_totals)}")
    print(f"\nPruning thresholds:")
    for pct, keep in sorted(keep_thresholds.items()):
        print(f"  {pct}% coverage: keep {keep}/256 experts (prune {256-keep})")
    print(f"\nSaved to: {output_path}")


def main():
    parser = argparse.ArgumentParser(description="Profile MoE expert activation for compaction")
    parser.add_argument("--model", default=MODEL_PATH, help="Path to model directory")
    parser.add_argument("--output", default="/tmp/expert_activation_profile.json", help="Output JSON path")
    parser.add_argument("--prompts", default=None, help="Optional JSONL file with custom prompts")
    parser.add_argument("--device", default="cuda", help="Device (cuda or cpu)")
    args = parser.parse_args()

    prompts = DEFAULT_PROMPTS
    if args.prompts:
        with open(args.prompts) as f:
            prompts = [json.loads(line)["text"] for line in f]

    print(f"=== Expert Activation Profiler ===")
    print(f"Model: {args.model}")
    print(f"Prompts: {len(prompts)}")
    print(f"Output: {args.output}")
    print()

    activation_counts, total_tokens = profile_experts(args.model, prompts, args.device)
    analyze_and_save(activation_counts, total_tokens, args.output)


if __name__ == "__main__":
    main()
