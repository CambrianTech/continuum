"""Runtime expert activation profiling for MoE compaction."""
import torch
import json
import time
import sys
from collections import defaultdict
from transformers import AutoModelForCausalLM, AutoTokenizer

MODEL = "/home/joel/.continuum/models/qwen3.5-35b-a3b-opus"

PROMPTS = [
    "Write a TypeScript function that implements a rate limiter using the token bucket algorithm.",
    "Fix this Rust lifetime error and explain why it occurs.",
    "Take a screenshot of the chat widget and describe what you see.",
    "The sidebar CSS is broken on mobile. Find the breakpoint issue.",
    "Compare B-tree vs hash index for our chat_messages table.",
    "Create a new Lit element widget with shadow DOM for displaying metrics.",
    "Implement WebSocket reconnection with exponential backoff in TypeScript.",
    "Good morning team! What should we work on today?",
    "Search the codebase for deprecated API calls and list affected files.",
    "Design a dashboard card with sparkline chart using CSS grid.",
]

print("Loading tokenizer...", flush=True)
tokenizer = AutoTokenizer.from_pretrained(MODEL, trust_remote_code=True)

print("Loading model (bf16, auto device map)...", flush=True)
model = AutoModelForCausalLM.from_pretrained(
    MODEL, dtype=torch.bfloat16, device_map="auto",
    trust_remote_code=True, max_memory={0: "30GiB", "cpu": "48GiB"}
)
model.eval()
print(f"GPU: {torch.cuda.memory_allocated()/1e9:.1f}GB", flush=True)

# Install hooks on MoE gate modules
activation_log = defaultdict(lambda: defaultdict(int))
total_decisions = defaultdict(int)

def make_gate_hook(layer_idx):
    def hook(module, input, output):
        if isinstance(input, tuple) and len(input) > 0:
            hidden = input[0]
            if hidden.dim() == 3 and hasattr(module, "weight"):
                with torch.no_grad():
                    logits = torch.matmul(hidden.float(), module.weight.float().t())
                    topk = logits.topk(8, dim=-1).indices
                    for eid in topk.reshape(-1).tolist():
                        activation_log[layer_idx][eid] += 1
                    total_decisions[layer_idx] += hidden.shape[0] * hidden.shape[1]
    return hook

hooks = []
for name, mod in model.named_modules():
    if name.endswith(".mlp.gate") and hasattr(mod, "weight"):
        if mod.weight is not None and mod.weight.shape[0] == 256:
            layer_idx = int(name.split(".")[2])
            hooks.append(mod.register_forward_hook(make_gate_hook(layer_idx)))

print(f"Hooked {len(hooks)} MoE gate modules", flush=True)

if len(hooks) == 0:
    print("ERROR: No gate modules hooked. Listing candidates:", flush=True)
    for name, mod in model.named_modules():
        if "gate" in name.lower() and hasattr(mod, "weight") and mod.weight is not None:
            print(f"  {name}: {list(mod.weight.shape)}", flush=True)
    sys.exit(1)

# Run prompts
for i, prompt in enumerate(PROMPTS):
    tokens = tokenizer(prompt, return_tensors="pt", truncation=True, max_length=256)
    tokens = {k: v.to(model.device) for k, v in tokens.items()}
    with torch.no_grad():
        t0 = time.time()
        out = model.generate(**tokens, max_new_tokens=64, do_sample=False)
        dt = time.time() - t0
    gen = out.shape[1] - tokens["input_ids"].shape[1]
    print(f"  [{i+1}/{len(PROMPTS)}] {gen} tok in {dt:.1f}s: {prompt[:50]}...", flush=True)

for h in hooks:
    h.remove()

# Analyze
total = sum(sum(v.values()) for v in activation_log.values())
if total == 0:
    print("WARNING: Zero activations recorded. Hooks may not have fired.", flush=True)
    sys.exit(1)

expert_totals = defaultdict(int)
for layer, experts in activation_log.items():
    for eid, count in experts.items():
        expert_totals[eid] += count

ranked = sorted(expert_totals.items(), key=lambda x: x[1], reverse=True)

print(f"\n{'='*50}", flush=True)
print(f"RUNTIME EXPERT ACTIVATION RESULTS", flush=True)
print(f"{'='*50}", flush=True)
print(f"Total activations: {total:,}", flush=True)
print(f"Experts activated: {len(expert_totals)}/256", flush=True)
print(f"Never activated: {256 - len(expert_totals)}", flush=True)

for target in [80, 90, 95, 99]:
    cum = 0
    for i, (eid, count) in enumerate(ranked):
        cum += count
        if cum / total * 100 >= target:
            print(f"  {target}% coverage: keep {i+1} experts, prune {256-i-1}", flush=True)
            break

print(f"\nTop 10 experts:", flush=True)
for eid, count in ranked[:10]:
    print(f"  Expert {eid}: {count} activations ({count/total*100:.1f}%)", flush=True)

print(f"\nBottom 10:", flush=True)
for eid, count in ranked[-10:]:
    print(f"  Expert {eid}: {count} activations ({count/total*100:.3f}%)", flush=True)

result = {
    "total": total,
    "activated": len(expert_totals),
    "ranked": [{"id": eid, "count": c, "pct": round(c/total*100, 4)} for eid, c in ranked],
}
with open("/tmp/runtime_activation_profile.json", "w") as f:
    json.dump(result, f, indent=2)
print(f"\nSaved to /tmp/runtime_activation_profile.json", flush=True)
