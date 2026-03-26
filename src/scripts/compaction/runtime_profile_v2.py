"""Runtime expert activation profiling v2 — uses output_router_logits=True."""
import torch
import json
import time
from collections import defaultdict
from transformers import AutoModelForCausalLM, AutoTokenizer

MODEL = "/home/joel/.continuum/models/qwen3.5-35b-a3b-opus"

PROMPTS = [
    "Write a TypeScript function that implements a rate limiter.",
    "Fix this Rust lifetime error and explain why.",
    "The sidebar CSS is broken on mobile. Find the breakpoint issue.",
    "Compare B-tree vs hash index for our chat_messages table.",
    "Good morning team! What should we work on today?",
]

print("Loading tokenizer...", flush=True)
tokenizer = AutoTokenizer.from_pretrained(MODEL, trust_remote_code=True)

print("Loading model with output_router_logits=True...", flush=True)

# Enable router logit output in config
import json as json_mod
config_path = f"{MODEL}/config.json"
with open(config_path) as f:
    config = json_mod.load(f)
config["text_config"]["output_router_logits"] = True
with open(config_path, "w") as f:
    json_mod.dump(config, f, indent=4)

model = AutoModelForCausalLM.from_pretrained(
    MODEL, dtype=torch.bfloat16, device_map="auto",
    trust_remote_code=True, max_memory={0: "30GiB", "cpu": "48GiB"}
)
model.eval()
print(f"GPU: {torch.cuda.memory_allocated()/1e9:.1f}GB", flush=True)

expert_counts = defaultdict(int)
total_tokens = 0

for i, prompt in enumerate(PROMPTS):
    tokens = tokenizer(prompt, return_tensors="pt", truncation=True, max_length=256)
    tokens = {k: v.to(model.device) for k, v in tokens.items()}

    with torch.no_grad():
        t0 = time.time()
        outputs = model(**tokens, output_router_logits=True)
        dt = time.time() - t0

    # Check if router_logits are in the output
    if hasattr(outputs, 'router_logits') and outputs.router_logits is not None:
        for layer_idx, logits in enumerate(outputs.router_logits):
            if logits is not None:
                # logits shape: (batch, seq_len, num_experts)
                topk = logits.topk(8, dim=-1).indices
                for eid in topk.reshape(-1).tolist():
                    expert_counts[eid] += 1
                total_tokens += logits.shape[0] * logits.shape[1]
        print(f"  [{i+1}/{len(PROMPTS)}] {dt:.1f}s, {total_tokens} routing decisions so far", flush=True)
    else:
        # Try other attribute names
        for attr in ['aux_loss', 'moe_logits', 'gate_logits']:
            if hasattr(outputs, attr):
                print(f"  Found attr: {attr}", flush=True)
        print(f"  [{i+1}/{len(PROMPTS)}] {dt:.1f}s — no router_logits in output. Keys: {[k for k in outputs.keys() if k != 'logits']}", flush=True)

# Restore config
config["text_config"]["output_router_logits"] = False
with open(config_path, "w") as f:
    json_mod.dump(config, f, indent=4)

if total_tokens == 0:
    print("WARNING: No routing data captured.", flush=True)
    print("Model output keys:", list(outputs.keys()) if outputs else "none", flush=True)
else:
    ranked = sorted(expert_counts.items(), key=lambda x: x[1], reverse=True)
    total = sum(expert_counts.values())
    print(f"\n=== RESULTS ===", flush=True)
    print(f"Total routing decisions: {total:,}", flush=True)
    print(f"Experts activated: {len(expert_counts)}/256", flush=True)

    for pct in [80, 90, 95, 99]:
        s = 0
        for idx, (eid, count) in enumerate(ranked):
            s += count
            if s / total * 100 >= pct:
                print(f"  {pct}%: keep {idx+1}, prune {256-idx-1}", flush=True)
                break

    result = {
        "total": total,
        "activated": len(expert_counts),
        "ranked": [{"id": eid, "count": c, "pct": round(c/total*100, 4)} for eid, c in ranked],
    }
    with open("/tmp/runtime_activation_profile.json", "w") as f:
        json.dump(result, f, indent=2)
    print(f"Saved to /tmp/runtime_activation_profile.json", flush=True)
