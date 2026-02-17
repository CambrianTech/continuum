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
    """
    print(f"\n🤖 Loading base model: {base_model}")
    print(f"   Quantization: {'QLoRA ' + str(quantize_bits) + '-bit' if quantize else 'disabled'}")

    # Load tokenizer
    tokenizer = AutoTokenizer.from_pretrained(base_model, trust_remote_code=True)

    # Add padding token if missing
    if tokenizer.pad_token is None:
        tokenizer.pad_token = tokenizer.eos_token

    # QLoRA: Try 4-bit quantization on any device that supports BitsAndBytes
    use_qlora = False
    if quantize:
        try:
            if quantize_bits == 4:
                bnb_config = BitsAndBytesConfig(
                    load_in_4bit=True,
                    bnb_4bit_quant_type="nf4",
                    bnb_4bit_compute_dtype=torch.bfloat16 if device == "cuda" else torch.float16,
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
                trust_remote_code=True
            )
            model = prepare_model_for_kbit_training(model)
            use_qlora = True
            print(f"✅ QLoRA {quantize_bits}-bit quantization active")
        except Exception as e:
            print(f"⚠️  QLoRA failed ({e}), falling back to full precision")

    if not use_qlora:
        # Fallback: full precision (float16 on GPU, float32 on CPU)
        dtype = torch.float16 if device in ("mps", "cuda") else torch.float32
        model = AutoModelForCausalLM.from_pretrained(
            base_model,
            torch_dtype=dtype,
            device_map={"": device} if device != "cuda" else "auto",
            trust_remote_code=True
        )

    print(f"✅ Model loaded successfully")
    return model, tokenizer


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
    print(f"\n🎯 Starting training...")
    print(f"   Examples: {len(dataset)}")
    print(f"   Epochs: {config['epochs']}")
    print(f"   Batch size: {config['batchSize']}")
    print(f"   Learning rate: {config['learningRate']}")

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

    # Training arguments
    output_dir = config['outputDir']
    training_args = TrainingArguments(
        output_dir=output_dir,
        per_device_train_batch_size=config['batchSize'],
        gradient_accumulation_steps=4,
        warmup_steps=5,
        num_train_epochs=config['epochs'],
        learning_rate=config['learningRate'],
        fp16=False,  # MPS doesn't support fp16
        bf16=False,
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
    model, tokenizer = load_model_and_tokenizer(
        config['baseModel'], device,
        quantize=config.get('quantize', True),
        quantize_bits=config.get('quantizeBits', 4)
    )

    # Step 4: Configure LoRA
    model = configure_lora(model, config['rank'], config['alpha'])

    # Step 5: Load training dataset
    dataset = load_dataset_from_jsonl(config['datasetPath'])

    # Step 6: Train LoRA adapter
    trainer = train(config, model, tokenizer, dataset, device)

    # Step 7: Save adapter weights
    save_adapter(model, tokenizer, args.output)

    print("\n" + "=" * 60)
    print("✅ LoRA Training: SUCCESS")
    print(f"   Adapter saved to: {args.output}")
    print(f"   Ready for inference with Transformers or Ollama")


if __name__ == "__main__":
    main()
