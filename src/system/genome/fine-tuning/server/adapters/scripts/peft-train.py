"""
Standard PyTorch + PEFT LoRA Training Script

Purpose: Train LoRA adapters using standard PyTorch/Transformers/PEFT
Philosophy: Works everywhere - Apple Silicon MPS, CUDA, CPU

Usage:
    python peft-train.py --config config.json --output output_dir

Requirements:
    torch, transformers, datasets, trl, peft, accelerate
"""

import argparse
import json
import os
import sys
import time
from pathlib import Path
from typing import Dict, Any

# Check dependencies
try:
    import torch
    from transformers import (
        AutoModelForCausalLM,
        AutoTokenizer,
        TrainingArguments,
        BitsAndBytesConfig
    )
    from datasets import load_dataset
    from trl import SFTTrainer
    from peft import LoraConfig, get_peft_model, prepare_model_for_kbit_training
    import accelerate
    print("✅ All required libraries imported successfully")
except ImportError as e:
    print(f"❌ ERROR: Missing dependency: {e}")
    print("   Install with: pip install torch transformers datasets trl peft accelerate")
    sys.exit(1)


def report_memory(phase: str, device: str):
    """Emit a structured memory report line that the Rust sentinel parses.

    The sentinel watches stdout for lines starting with 'MEMORY_REPORT:' and
    emits a sentinel:{handle}:memory-report event so TrainingMemoryGuard can
    update the registered consumer with real memory data instead of estimates.
    """
    report = {
        "phase": phase,
        "device": device,
        "timestamp": time.time(),
        "allocated_bytes": 0,
        "peak_bytes": 0,
        "process_rss_bytes": 0,
    }

    # Device-specific memory
    if device == "mps" and hasattr(torch.mps, "current_allocated_memory"):
        report["allocated_bytes"] = torch.mps.current_allocated_memory()
        # MPS doesn't expose peak — use driver_allocated as upper bound
        if hasattr(torch.mps, "driver_allocated_memory"):
            report["peak_bytes"] = torch.mps.driver_allocated_memory()
        else:
            report["peak_bytes"] = report["allocated_bytes"]
    elif device == "cuda":
        report["allocated_bytes"] = torch.cuda.memory_allocated()
        report["peak_bytes"] = torch.cuda.max_memory_allocated()

    # Process-level RSS (works on all platforms)
    try:
        import resource
        rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        # macOS reports bytes, Linux reports KB
        if sys.platform == "darwin":
            report["process_rss_bytes"] = rss
        else:
            report["process_rss_bytes"] = rss * 1024
    except Exception:
        pass

    # Structured line — sentinel parses this prefix
    print(f"MEMORY_REPORT: {json.dumps(report)}", flush=True)


def load_config(config_path: str) -> Dict[str, Any]:
    """Load training configuration from JSON file."""
    print(f"📋 Loading config from: {config_path}")

    with open(config_path, 'r') as f:
        config = json.load(f)

    print(f"   Base model: {config['baseModel']}")
    print(f"   Dataset: {config['datasetPath']}")
    print(f"   LoRA rank: {config['rank']}")
    print(f"   LoRA alpha: {config['alpha']}")
    print(f"   Epochs: {config['epochs']}")
    print(f"   Learning rate: {config['learningRate']}")
    print(f"   Batch size: {config['batchSize']}")
    print(f"   QLoRA: {config.get('quantize', True)} ({config.get('quantizeBits', 4)}-bit)")

    return config


def detect_device():
    """Detect best available device."""
    if torch.cuda.is_available():
        device = "cuda"
        print(f"✅ Using CUDA GPU: {torch.cuda.get_device_name(0)}")
    elif torch.backends.mps.is_available():
        device = "mps"
        print(f"✅ Using Apple Silicon MPS")
    else:
        device = "cpu"
        print(f"⚠️  Using CPU (training will be slow)")

    return device


def load_model_and_tokenizer(base_model: str, device: str, quantize: bool = True, quantize_bits: int = 4):
    """Load base model and tokenizer with QLoRA quantization when available.

    QLoRA strategy: quantize the base model to 4-bit NF4 so you can train the
    LARGEST model that fits on hardware. LoRA weights stay full precision.
    A 3B model in 4-bit fits in ~2GB VRAM, 8B in ~5GB.

    Returns (model, tokenizer, quantization_info) where quantization_info
    records what actually happened (may differ from requested if fallback occurred).
    """
    print(f"\n🤖 Loading base model: {base_model}")
    print(f"   Quantization: {'QLoRA ' + str(quantize_bits) + '-bit' if quantize else 'disabled'}")

    # Track what quantization actually happened
    quantization_info = {
        "enabled": False,
        "bits": quantize_bits,
        "type": "nf4" if quantize_bits == 4 else "int8",
        "doubleQuant": quantize_bits == 4,
        "computeDtype": "bfloat16" if device == "cuda" else "float16",
    }

    # Load tokenizer
    tokenizer = AutoTokenizer.from_pretrained(base_model, trust_remote_code=True)

    # Add padding token if missing
    if tokenizer.pad_token is None:
        tokenizer.pad_token = tokenizer.eos_token

    # QLoRA: Try requested quantization, then 8-bit fallback, then full precision
    use_qlora = False
    actual_bits = quantize_bits
    if quantize:
        try:
            if quantize_bits == 4:
                compute_dtype = torch.bfloat16 if device == "cuda" else torch.float16
                bnb_config = BitsAndBytesConfig(
                    load_in_4bit=True,
                    bnb_4bit_quant_type="nf4",
                    bnb_4bit_compute_dtype=compute_dtype,
                    bnb_4bit_use_double_quant=True,
                )
            else:
                bnb_config = BitsAndBytesConfig(
                    load_in_8bit=True,
                )

            model = AutoModelForCausalLM.from_pretrained(
                base_model,
                quantization_config=bnb_config,
                device_map="auto",
                trust_remote_code=True,
                low_cpu_mem_usage=True,
            )
            model = prepare_model_for_kbit_training(model)
            use_qlora = True
            actual_bits = quantize_bits
            print(f"✅ QLoRA {quantize_bits}-bit quantization active")
        except Exception as e:
            print(f"⚠️  QLoRA {quantize_bits}-bit failed ({e})")
            # Fallback: try 8-bit if we were trying 4-bit
            if quantize_bits == 4:
                try:
                    bnb_config = BitsAndBytesConfig(load_in_8bit=True)
                    model = AutoModelForCausalLM.from_pretrained(
                        base_model,
                        quantization_config=bnb_config,
                        device_map="auto",
                        trust_remote_code=True,
                        low_cpu_mem_usage=True,
                    )
                    model = prepare_model_for_kbit_training(model)
                    use_qlora = True
                    actual_bits = 8
                    print(f"✅ QLoRA 8-bit fallback active")
                except Exception as e2:
                    print(f"⚠️  QLoRA 8-bit also failed ({e2}), falling back to full precision")

    if not use_qlora:
        # Fallback: full precision (float16 on GPU, float32 on CPU)
        dtype = torch.float16 if device in ("mps", "cuda") else torch.float32
        model = AutoModelForCausalLM.from_pretrained(
            base_model,
            torch_dtype=dtype,
            device_map={"": device} if device != "cuda" else "auto",
            trust_remote_code=True,
            low_cpu_mem_usage=True,
        )

    # Record actual quantization state
    quantization_info["enabled"] = use_qlora
    quantization_info["bits"] = actual_bits
    quantization_info["type"] = "nf4" if actual_bits == 4 else "int8"
    quantization_info["doubleQuant"] = actual_bits == 4 and use_qlora

    # Enable gradient checkpointing — trades compute for memory.
    # Without this, activation tensors for all layers are held in RAM during backprop.
    # For a 3B model this can be 4-8GB of activations alone.
    if hasattr(model, 'gradient_checkpointing_enable'):
        model.gradient_checkpointing_enable()
        print(f"✅ Gradient checkpointing enabled (saves ~50% activation memory)")

    print(f"✅ Model loaded successfully")
    return model, tokenizer, quantization_info


def configure_lora(model, rank: int, alpha: int):
    """Configure and apply LoRA to model."""
    print(f"\n🔧 Applying LoRA configuration (rank={rank}, alpha={alpha})")

    lora_config = LoraConfig(
        r=rank,
        lora_alpha=alpha,
        target_modules=[
            "q_proj", "k_proj", "v_proj", "o_proj",
            "gate_proj", "up_proj", "down_proj"
        ],
        lora_dropout=0.05,
        bias="none",
        task_type="CAUSAL_LM"
    )

    model = get_peft_model(model, lora_config)
    model.print_trainable_parameters()

    print(f"✅ LoRA configuration applied")
    return model


def load_dataset_from_jsonl(dataset_path: str):
    """Load training dataset from JSONL file."""
    print(f"\n📂 Loading dataset from: {dataset_path}")

    if not os.path.exists(dataset_path):
        raise FileNotFoundError(f"Dataset not found: {dataset_path}")

    dataset = load_dataset("json", data_files=dataset_path, split="train")

    print(f"✅ Dataset loaded: {len(dataset)} examples")

    if len(dataset) > 0:
        print(f"   First example: {str(dataset[0])[:200]}...")

    return dataset


def format_chat_template(example, tokenizer):
    """Format training example using chat template."""
    messages = example["messages"]

    # Use tokenizer's chat template
    text = tokenizer.apply_chat_template(
        messages,
        tokenize=False,
        add_generation_prompt=False
    )

    return {"text": text}


def train(config: Dict[str, Any], model, tokenizer, dataset, device: str):
    """Execute LoRA training."""
    num_examples = len(dataset)
    batch_size = config['batchSize']
    num_epochs = config['epochs']
    learning_rate = config['learningRate']

    # Dynamic gradient accumulation: target effective batch ~16 for large datasets,
    # but for small datasets (< 32 examples), accumulate less so we get enough optimizer steps.
    # With 20 examples and batch_size=4: steps_per_epoch=5
    # grad_accum=1 → 5 optimizer steps/epoch → 15 total (3 epochs) — plenty of learning
    # grad_accum=4 → 1 optimizer step/epoch → 3 total — all in warmup, no learning!
    steps_per_epoch = max(1, num_examples // batch_size)
    if steps_per_epoch <= 8:
        gradient_accumulation = 1  # Small dataset: every mini-batch is an optimizer step
    elif steps_per_epoch <= 32:
        gradient_accumulation = 2  # Medium dataset
    else:
        gradient_accumulation = 4  # Large dataset: standard accumulation

    total_optimizer_steps = (steps_per_epoch // gradient_accumulation) * num_epochs

    # Dynamic warmup: 10% of total steps, minimum 1, cap at 10
    # Never let warmup consume >30% of training (for tiny datasets)
    warmup = max(1, min(10, total_optimizer_steps // 10))
    if warmup > total_optimizer_steps * 0.3:
        warmup = max(1, int(total_optimizer_steps * 0.1))

    print(f"\n🎯 Starting training...")
    print(f"   Examples: {num_examples}")
    print(f"   Epochs: {num_epochs}")
    print(f"   Batch size: {batch_size}")
    print(f"   Learning rate: {learning_rate}")
    print(f"   Gradient accumulation: {gradient_accumulation} (effective batch={batch_size * gradient_accumulation})")
    print(f"   Steps/epoch: {steps_per_epoch}, optimizer steps/epoch: {steps_per_epoch // gradient_accumulation}")
    print(f"   Total optimizer steps: {total_optimizer_steps}")
    print(f"   Warmup steps: {warmup}")

    # Formatting function for TRL 0.24+
    def formatting_func(example):
        """Format training example using chat template."""
        messages = example["messages"]
        text = tokenizer.apply_chat_template(
            messages,
            tokenize=False,
            add_generation_prompt=False
        )
        return text

    # Mixed precision: bf16 on CUDA (halves activation memory), disabled on MPS/CPU.
    # MPS doesn't support fp16/bf16 training. CPU has no benefit.
    use_bf16 = (device == "cuda" and torch.cuda.is_bf16_supported())
    use_fp16 = (device == "cuda" and not use_bf16)
    if use_bf16:
        print(f"   Mixed precision: bf16 (halves activation memory)")
    elif use_fp16:
        print(f"   Mixed precision: fp16")
    else:
        print(f"   Mixed precision: disabled ({device})")

    # Training arguments
    output_dir = config['outputDir']
    training_args = TrainingArguments(
        output_dir=output_dir,
        per_device_train_batch_size=batch_size,
        gradient_accumulation_steps=gradient_accumulation,
        warmup_steps=warmup,
        num_train_epochs=num_epochs,
        learning_rate=learning_rate,
        fp16=use_fp16,
        bf16=use_bf16,
        gradient_checkpointing=True,
        logging_steps=1,
        optim="adamw_torch",
        weight_decay=0.01,
        lr_scheduler_type="linear",
        seed=42,
        report_to="none",
        save_strategy="epoch",
        save_total_limit=1,
    )

    # SFT Trainer (TRL 0.24+ simplified API)
    trainer = SFTTrainer(
        model=model,
        processing_class=tokenizer,
        train_dataset=dataset,
        formatting_func=formatting_func,
        args=training_args,
    )

    # Train!
    print(f"🚀 Training started...")
    trainer_stats = trainer.train()

    print(f"✅ Training complete!")
    print(f"   Final loss: {trainer_stats.training_loss:.4f}")
    print(f"   Training time: {trainer_stats.metrics['train_runtime']:.2f}s")
    print(f"   Examples/second: {trainer_stats.metrics.get('train_samples_per_second', 0):.2f}")

    return trainer


def save_adapter(model, tokenizer, output_dir: str):
    """Save trained LoRA adapter."""
    print(f"\n💾 Saving adapter to: {output_dir}")

    # Save LoRA adapter weights
    model.save_pretrained(output_dir)
    tokenizer.save_pretrained(output_dir)

    print(f"✅ Adapter saved successfully")
    print(f"   Files: adapter_config.json, adapter_model.safetensors")


def main():
    """Main training pipeline."""
    parser = argparse.ArgumentParser(description="Standard PyTorch LoRA Training")
    parser.add_argument("--config", required=True, help="Path to config JSON")
    parser.add_argument("--output", required=True, help="Output directory for adapter")

    args = parser.parse_args()

    print("🧬 Standard PyTorch + PEFT LoRA Training")
    print("=" * 60)

    # Step 1: Load configuration
    config = load_config(args.config)
    config['outputDir'] = args.output

    # Step 2: Detect device
    device = detect_device()

    # Step 3: Load base model and tokenizer (QLoRA quantization enabled by default)
    model, tokenizer, quantization_info = load_model_and_tokenizer(
        config['baseModel'], device,
        quantize=config.get('quantize', True),
        quantize_bits=config.get('quantizeBits', 4)
    )

    # Step 4: Configure LoRA
    model = configure_lora(model, config['rank'], config['alpha'])

    # Report memory after model + LoRA are loaded (the big allocation)
    report_memory("model_loaded", device)

    # Step 5: Load training dataset
    dataset = load_dataset_from_jsonl(config['datasetPath'])

    # Step 6: Train LoRA adapter
    report_memory("pre_training", device)
    trainer = train(config, model, tokenizer, dataset, device)
    report_memory("post_training", device)

    # Step 7: Save adapter weights
    save_adapter(model, tokenizer, args.output)

    # Step 8: Write quantization metadata alongside adapter
    quant_info_path = os.path.join(args.output, "quantization_info.json")
    with open(quant_info_path, 'w') as f:
        json.dump(quantization_info, f, indent=2)
    print(f"📊 Quantization info written to: {quant_info_path}")

    print("\n" + "=" * 60)
    print("✅ LoRA Training: SUCCESS")
    print(f"   Adapter saved to: {args.output}")
    print(f"   QLoRA: {'enabled (' + str(quantization_info['bits']) + '-bit ' + quantization_info['type'] + ')' if quantization_info['enabled'] else 'disabled (full precision)'}")
    print(f"   Ready for inference with Transformers or Ollama")


if __name__ == "__main__":
    try:
        main()
    except torch.cuda.OutOfMemoryError:
        print("\n❌ CUDA OUT OF MEMORY")
        print("   Try: smaller batch size, lower rank, or enable quantization")
        torch.cuda.empty_cache()
        sys.exit(137)
    except RuntimeError as e:
        if "out of memory" in str(e).lower() or "mps" in str(e).lower():
            print(f"\n❌ OUT OF MEMORY: {e}")
            print("   Try: smaller batch size, lower rank, or smaller model")
            sys.exit(137)
        raise
