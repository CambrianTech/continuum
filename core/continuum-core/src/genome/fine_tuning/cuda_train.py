"""CUDA QLoRA kernel for the Rust-owned training job.

Recovered from the legacy PEFT path: no CPU/full-precision fallback, no remote
code execution, no stdout protocol, no hidden sequence truncation. Rust owns
admission and process lifetime. Files carry prepared input and final receipts.
"""
import argparse
import json
import math
import os
import time
from importlib.metadata import PackageNotFoundError, version
from pathlib import Path


def write_json(path, value):
    path = Path(path)
    temporary = path.with_suffix(path.suffix + ".partial")
    temporary.write_text(json.dumps(value, allow_nan=False), encoding="utf-8")
    os.replace(temporary, path)


def model_class(config):
    from transformers import AutoModelForCausalLM, AutoModelForImageTextToText
    # Some multimodal configs also register a text-only CausalLM projection.
    # Preserve the actual wrapper and parameter paths used by the serving base.
    if type(config) in AutoModelForImageTextToText._model_mapping:
        return AutoModelForImageTextToText
    if type(config) in AutoModelForCausalLM._model_mapping:
        return AutoModelForCausalLM
    raise ValueError(f"no installed Transformers trainer for {config.model_type}")


# Per-vocab-entry bytes the loss peak costs, per supervised token.
#
# transformers' ForCausalLMLoss upcasts the WHOLE logits tensor
# (`loss_utils.py`: `logits = logits.float()`), so the peak holds the bf16
# logits (2), their fp32 copy (4), and the fp32 gradient of that copy (4) at
# once, plus the shifted copy cross-entropy makes internally.
#
# This was 8, and 8 is what let a plan be admitted and then die inside its own
# admission: the 5090, 2026-09-25 00:23Z, Kimi's job b73456e6 — the governor
# admitted the plan, the trainer honoured it ("28.91 GiB allowed"), held
# 27.36 GiB, and the final `logits.float()` asked for 1.89 GiB more. A planner
# whose estimate is under the true peak is not a budget; it is a guess that
# gets a job killed after it has paid for the weights. Erring HIGH costs a
# smaller micro-batch; erring low costs the whole run.
LOGITS_BYTES_PER_TOKEN = 12

# The loss holds logits for at most this many positions at once (per example in the
# microbatch). Cormac on the Stage 2 plan (2026-09-26): Kimi's 27B plan asked 33.07 GB
# on a 32.6 GB card, and 6.10 GB of it was the `logits` term — the whole [seq x vocab]
# tensor materialised for the loss. Applying the output head and the loss per chunk
# of positions, under activation checkpointing so a chunk's logits are freed after
# its loss and recomputed on backward, bounds that term to one chunk whatever the
# window: at 512 positions and a 152k vocab, ~0.9 GB instead of 6.1. It does not make
# co-residency with a serving lane fit (weights 24.15 + activations 2.73 + the lane's
# 29.6 is still over the card); it is headroom for either path.
LOGITS_CHUNK_TOKENS = 512


def chunked_causal_lm_loss(model, batch, chunk=LOGITS_CHUNK_TOKENS):
    """Mean next-token cross-entropy over the supervised targets — the same objective
    as the model's own loss — computed without ever holding the [seq x vocab] logits.
    The base runs once for the hidden states; the output head and the loss run per
    chunk of positions under checkpointing. Works through a PEFT wrapper: the LoRA
    layers live inside the base's modules, so running the base runs them."""
    import torch
    import torch.nn.functional as F
    from torch.utils.checkpoint import checkpoint
    base = model.get_base_model() if hasattr(model, "get_base_model") else model
    body, head = base.model, base.get_output_embeddings()
    inputs = {name: value for name, value in batch.items() if name != "labels"}
    hidden = body(**inputs).last_hidden_state[:, :-1]
    targets = batch["labels"][:, 1:]
    count = int((targets != -100).sum())
    if count == 0:
        raise ValueError("no supervised targets in batch")

    def chunk_loss(h, t):
        logits = head(h).float()
        return F.cross_entropy(logits.reshape(-1, logits.shape[-1]), t.reshape(-1),
                               ignore_index=-100, reduction="sum")

    total = hidden.new_zeros((), dtype=torch.float32)
    for start in range(0, hidden.shape[1], chunk):
        h, t = hidden[:, start:start + chunk], targets[:, start:start + chunk]
        if bool((t != -100).any()):
            total = total + checkpoint(chunk_loss, h, t, use_reentrant=False)
    return total / count


def plan(spec, output):
    import torch
    from accelerate import init_empty_weights
    from transformers import AutoConfig
    if not torch.cuda.is_available():
        raise RuntimeError("CUDA trainer requires a working CUDA PyTorch build")
    config = AutoConfig.from_pretrained(spec["baseModel"], trust_remote_code=False)
    with init_empty_weights():
        model = model_class(config).from_config(config, trust_remote_code=False)
    parameters = sum(p.numel() for p in model.parameters())
    # Transformers keeps the output head unquantized. Do not count its bytes
    # as NF4; prepare_model_for_kbit_training promotes these weights to fp32.
    output_head = model.get_output_embeddings()
    linear = sum(m.weight.numel() for m in model.modules()
                 if isinstance(m, torch.nn.Linear) and m is not output_head)
    targets = spec["lora"]["targetModules"]
    selected = [(n, m) for n, m in model.named_modules()
                if isinstance(m, torch.nn.Linear) and
                (any(n.endswith("." + target) or n == target for target in targets))]
    missing = [target for target in targets
               if not any(name == target or name.endswith("." + target) for name, _ in selected)]
    if not selected or missing:
        raise ValueError(f"LoRA targets missing from the actual base: {missing or targets}")
    rank = spec["lora"]["rank"]
    lora_parameters = sum(rank * (m.in_features + m.out_features) for _, m in selected)
    text = config.get_text_config() if hasattr(config, "get_text_config") else config
    schedule = spec["schedule"]
    # Backend-owned microbatching preserves the requested effective batch and
    # sequence window. The resource governor still atomically admits the plan.
    weights = math.ceil(linear * (0.5 + 4 / 64)) + (parameters - linear) * 4
    optimizer = lora_parameters * 16
    free, _ = torch.cuda.mem_get_info()
    available = min(free, spec.get("availableBytes", free))
    sequence = schedule["sequenceLength"]
    # The logits peak is one chunk of positions, not the whole window (see
    # `chunked_causal_lm_loss`); the activation term still scales with the window.
    per_example = (sequence * int(text.hidden_size) * (int(text.num_hidden_layers) + 1) * 4
                   + min(sequence, LOGITS_CHUNK_TOKENS) * int(text.vocab_size) * LOGITS_BYTES_PER_TOKEN)
    # CUDACachingAllocator large slabs: rounding plus one working slab.
    slab = 20 * 1024 * 1024
    logical_budget = max(0, (available // slab - 1) * slab)
    # Planning describes required capacity, not permission to allocate it. Even
    # with no free memory, return a one-example plan for the owner to queue.
    micro = min(schedule["batchSize"], max(1, (logical_budget - weights - optimizer) // per_example))
    tokens = micro * sequence
    activations = tokens * int(text.hidden_size) * (int(text.num_hidden_layers) + 1) * 4
    logits = micro * min(sequence, LOGITS_CHUNK_TOKENS) * int(text.vocab_size) * LOGITS_BYTES_PER_TOKEN
    terms = dict(weights=weights, optimizer=optimizer, activations=activations, logits=logits)
    terms["allocator"] = slab + (-sum(terms.values()) % slab)
    write_json(output, {"memoryBytes": sum(terms.values()), "terms": terms,
                        "microBatchSize": micro, "effectiveBatchSize": schedule["batchSize"],
                        "revision": getattr(config, "_commit_hash", None),
                        "parameters": parameters, "loraParameters": lora_parameters})


def chunks(ids, prompt_length, sequence_length):
    """Retain every supervised next-token target; overlap one causal predecessor."""
    if sequence_length < 2:
        raise ValueError("sequenceLength must be at least two")
    for start in range(0, len(ids) - 1, sequence_length - 1):
        tokens = ids[start:start + sequence_length]
        labels = [-100 if start + i < prompt_length else token
                  for i, token in enumerate(tokens)]
        if any(label != -100 for label in labels[1:]):
            yield {"input_ids": tokens, "labels": labels}


def prepare_examples(examples, tokenizer, sequence_length):
    prepared = []
    for example in examples:
        if tokenizer.chat_template:
            messages = [{"role": "user", "content": example["prompt"]}]
            # Use the training template on both sides. A thinking model's
            # generation prompt may open a reasoning channel that its completed
            # assistant template closes differently. All assistant-owned template
            # tokens remain supervised; user/system tokens remain masked.
            prefix = tokenizer.apply_chat_template(
                messages, tokenize=True, return_dict=True, add_generation_prompt=False)["input_ids"]
            complete = tokenizer.apply_chat_template(
                messages + [{"role": "assistant", "content": example["completion"]}],
                tokenize=True, return_dict=True, add_generation_prompt=False)["input_ids"]
            if complete[:len(prefix)] != prefix:
                raise ValueError("chat template changes the assistant prefix; explicit model adapter required")
        else:
            prefix = tokenizer.encode(example["prompt"], add_special_tokens=True)
            complete = prefix + tokenizer.encode(example["completion"], add_special_tokens=False)
            if tokenizer.eos_token_id is not None:
                complete.append(tokenizer.eos_token_id)
        rows = list(chunks(complete, len(prefix), sequence_length))
        if not rows:
            raise ValueError(f"training example has no supervised targets (prefix={len(prefix)}, total={len(complete)})")
        prepared.extend(rows)
    return prepared


def train(spec, output):
    import torch
    from transformers import AutoConfig, AutoTokenizer, BitsAndBytesConfig
    from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training
    started = time.monotonic()
    if not torch.cuda.is_available():
        raise RuntimeError("CUDA is unavailable; refusing CPU fallback")
    budget = int(spec["memoryBytes"])
    free, total = torch.cuda.mem_get_info()
    if budget <= 0 or budget > free:
        raise RuntimeError(f"CUDA allocation needs {budget} bytes, currently free {free}")
    torch.cuda.set_per_process_memory_fraction(budget / total)
    torch.manual_seed(0)
    dtype = torch.bfloat16 if torch.cuda.is_bf16_supported() else torch.float16
    base, revision = spec["baseModel"], spec.get("revision")
    config = AutoConfig.from_pretrained(base, revision=revision, trust_remote_code=False)
    tokenizer = AutoTokenizer.from_pretrained(base, revision=revision, trust_remote_code=False)
    if tokenizer.pad_token_id is None:
        if tokenizer.eos_token_id is None:
            raise ValueError("tokenizer has neither padding nor EOS token")
        tokenizer.pad_token = tokenizer.eos_token
    examples = spec["dataset"]["examples"]
    fraction = spec["dataset"]["validationSplit"]
    if not 0 <= fraction <= 0.5:
        raise ValueError("validationSplit must lie in [0, 0.5]")
    count = max(1, math.ceil(len(examples) * fraction)) if fraction else 0
    if count >= len(examples):
        raise ValueError("validation requires separate training and validation examples")
    schedule, lora = spec["schedule"], spec["lora"]
    # Split whole experiences BEFORE chunking: overlapping context cannot leak.
    boundary = len(examples) - count
    training = prepare_examples(examples[:boundary], tokenizer, schedule["sequenceLength"])
    validation = prepare_examples(examples[boundary:], tokenizer, schedule["sequenceLength"])
    model = model_class(config).from_pretrained(
        base, revision=revision, config=config, trust_remote_code=False,
        quantization_config=BitsAndBytesConfig(load_in_4bit=True, bnb_4bit_quant_type="nf4",
            bnb_4bit_compute_dtype=dtype, bnb_4bit_use_double_quant=True),
        device_map={"": 0}, torch_dtype=dtype)
    model = prepare_model_for_kbit_training(model, use_gradient_checkpointing=True,
                                           gradient_checkpointing_kwargs={"use_reentrant": False})
    model.config.use_cache = False
    model.config.get_text_config().use_cache = False
    model = get_peft_model(model, LoraConfig(r=lora["rank"], lora_alpha=lora["alpha"],
        lora_dropout=lora["dropout"], target_modules=lora["targetModules"],
        bias="none", task_type="CAUSAL_LM"))
    parameters = [p for p in model.parameters() if p.requires_grad]
    optimizer = torch.optim.AdamW(parameters, lr=schedule["learningRate"])
    scaler = torch.amp.GradScaler("cuda", enabled=dtype == torch.float16)
    output.mkdir(parents=True, exist_ok=True)

    micro = int(spec["microBatchSize"])
    if not 1 <= micro <= schedule["batchSize"]:
        raise ValueError("invalid admitted microbatch")

    def batches(rows):
        for start in range(0, len(rows), micro):
            batch = rows[start:start + micro]
            length = max(len(row["input_ids"]) for row in batch)
            yield {name: torch.tensor([
                (row[name] if name != "attention_mask" else [1] * len(row["input_ids"])) +
                [padding] * (length - len(row["input_ids"])) for row in batch], device="cuda")
                for name, padding in [("input_ids", tokenizer.pad_token_id),
                                      ("labels", -100), ("attention_mask", 0)]}

    trained_tokens, step, final_loss = 0, 0, None
    with (output / "loss.jsonl").open("a", encoding="utf-8") as losses:
        for epoch in range(schedule["epochs"]):
            model.train()
            for start in range(0, len(training), schedule["batchSize"]):
                group = training[start:start + schedule["batchSize"]]
                target_count = sum(sum(label != -100 for label in row["labels"][1:]) for row in group)
                optimizer.zero_grad(set_to_none=True)
                weighted_loss = 0.0
                for batch in batches(group):
                    count = int((batch["labels"][:, 1:] != -100).sum())
                    with torch.autocast("cuda", dtype=dtype):
                        loss = chunked_causal_lm_loss(model, batch)
                    if not torch.isfinite(loss):
                        raise RuntimeError("non-finite training loss")
                    # Exact token weighting preserves the effective-batch objective,
                    # including the final partial microbatch and unequal lengths.
                    scaler.scale(loss * (count / target_count)).backward()
                    weighted_loss += float(loss.detach()) * count
                scaler.unscale_(optimizer)
                torch.nn.utils.clip_grad_norm_(parameters, math.inf, error_if_nonfinite=True)
                scaler.step(optimizer)
                scaler.update()
                step += 1
                trained_tokens += target_count
                final_loss = weighted_loss / target_count
                losses.write(json.dumps({"step": step, "epoch": epoch, "loss": final_loss}) + "\n")
                losses.flush()
    validation_loss = None
    if validation:
        model.eval()
        weighted, tokens = 0.0, 0
        with torch.no_grad(), torch.autocast("cuda", dtype=dtype):
            for batch in batches(validation):
                count = int((batch["labels"][:, 1:] != -100).sum())
                weighted += float(model(**batch).loss) * count
                tokens += count
        validation_loss = weighted / tokens
        if not math.isfinite(validation_loss):
            raise RuntimeError("non-finite validation loss")
    model.save_pretrained(output, safe_serialization=True)
    tokenizer.save_pretrained(output)
    write_json(output / "metrics.json", {"trainedTokens": trained_tokens, "finalLoss": final_loss,
        "finalValidationLoss": validation_loss, "wallClockMs": int((time.monotonic()-started)*1000),
        "costUsd": None})
    packages = {}
    for package in ["transformers", "peft", "bitsandbytes", "causal-conv1d", "flash-linear-attention"]:
        try:
            packages[package] = version(package)
        except PackageNotFoundError:
            packages[package] = None
    write_json(output / "training-provenance.json", {"baseModel": base, "revision": revision,
        "device": torch.cuda.get_device_name(), "cuda": torch.version.cuda,
        "peakAllocatedBytes": torch.cuda.max_memory_allocated(), "budgetBytes": budget,
        "quantization": "nf4-double", "steps": step, "microBatchSize": micro,
        "effectiveBatchSize": schedule["batchSize"], "trainingRows": len(training),
        "validationRows": len(validation), "torch": torch.__version__,
        "packages": packages, "modelClass": type(model.get_base_model()).__name__})


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--plan", action="store_true")
    args = parser.parse_args()
    spec = json.loads(args.config.read_text(encoding="utf-8"))
    try:
        (plan if args.plan else train)(spec, args.output)
    except BaseException as failure:
        # THE PEAK IS ONLY USEFUL WHEN IT KILLED THE RUN. `training-provenance.json`
        # records peakAllocatedBytes on success, which is exactly the case where
        # nobody needs it; the OOM above left a traceback and no numbers, so the
        # estimate that caused it could not be calibrated from its own failure.
        # Best-effort, and it never masks the original: the raise below is
        # unconditional ([[fallbacks-are-illegal-fail-loud]]).
        if not args.plan:
            try:
                import torch as measured
                write_json(args.output / "training-failure.json", {
                    "error": str(failure)[:2000],
                    "peakAllocatedBytes": (measured.cuda.max_memory_allocated()
                                           if measured.cuda.is_available() else None),
                    "budgetBytes": spec.get("memoryBytes"),
                    "microBatchSize": spec.get("microBatchSize"),
                    "sequenceLength": (spec.get("schedule") or {}).get("sequenceLength"),
                    "logitsBytesPerToken": LOGITS_BYTES_PER_TOKEN,
                })
            except BaseException:
                pass
        raise
