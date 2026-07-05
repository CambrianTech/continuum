"""
Unsloth LoRA Training Script - Phase 7.1

Purpose: Train LoRA adapters using Unsloth library with 2x speed and 70% less VRAM.
Philosophy: "Start simple, expand systematically" - MVP training that just needs to finish.

Usage:
    python unsloth-train.py --config config.json --output output_dir

What This Script Does:
- Loads base model with Unsloth optimizations
- Loads training dataset from JSONL
- Trains LoRA adapter with specified hyperparameters
- Saves adapter in Hugging Face format
- Exports to GGUF format for Ollama serving

Requirements:
    pip install unsloth transformers datasets torch

References:
- Unsloth docs: https://github.com/unslothai/unsloth
- GGUF export: https://github.com/ggerganov/llama.cpp
"""

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Dict, Any

# Check Unsloth availability
try:
    from unsloth import FastLanguageModel
    from unsloth import is_bfloat16_supported
    print("✅ Unsloth library imported successfully")
except ImportError:
    print("❌ ERROR: Unsloth not installed")
    print("   Install with: pip install unsloth")
    sys.exit(1)

# Check other dependencies
try:
    from datasets import load_dataset
    from transformers import TrainingArguments
    from trl import SFTTrainer
    print("✅ Required libraries imported successfully")
except ImportError as e:
    print(f"❌ ERROR: Missing dependency: {e}")
    print("   Install with: pip install transformers datasets trl")
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

    return config


def load_model(base_model: str, rank: int, alpha: int):
    """Load base model with Unsloth optimizations and LoRA configuration."""
    print(f"\n🤖 Loading base model: {base_model}")
    print(f"   LoRA config: rank={rank}, alpha={alpha}")

    # Unsloth supports 4-bit quantization for memory efficiency
    max_seq_length = 2048  # Standard context window
    dtype = None  # Auto-detect based on GPU (float16 or bfloat16)
    load_in_4bit = True  # Use 4-bit quantization (70% less VRAM)

    model, tokenizer = FastLanguageModel.from_pretrained(
        model_name=base_model,
        max_seq_length=max_seq_length,
        dtype=dtype,
        load_in_4bit=load_in_4bit,
    )

    print(f"✅ Model loaded successfully")
    print(f"   Using {'bfloat16' if is_bfloat16_supported() else 'float16'} precision")
    print(f"   4-bit quantization: {load_in_4bit}")

    # Apply LoRA configuration
    model = FastLanguageModel.get_peft_model(
        model,
        r=rank,  # LoRA rank
        target_modules=["q_proj", "k_proj", "v_proj", "o_proj",
                        "gate_proj", "up_proj", "down_proj"],
        lora_alpha=alpha,
        lora_dropout=0,  # Optimized for 0 dropout
        bias="none",  # No bias for LoRA layers
        use_gradient_checkpointing="unsloth",  # Unsloth's optimized checkpointing
        random_state=42,
        use_rslora=False,  # Rank-stabilized LoRA (optional)
        loftq_config=None,  # LoftQ quantization (optional)
    )

    print(f"✅ LoRA configuration applied")

    return model, tokenizer


def load_dataset_from_jsonl(dataset_path: str):
    """Load training dataset from JSONL file."""
    print(f"\n📂 Loading dataset from: {dataset_path}")

    if not os.path.exists(dataset_path):
        raise FileNotFoundError(f"Dataset not found: {dataset_path}")

    # Load JSONL using Hugging Face datasets
    dataset = load_dataset("json", data_files=dataset_path, split="train")

    print(f"✅ Dataset loaded: {len(dataset)} examples")

    # Print first example for debugging
    if len(dataset) > 0:
        print(f"   First example: {str(dataset[0])[:200]}...")

    return dataset


def format_chat_template(example, tokenizer):
    """Format training example using chat template."""
    # Standard chat completions format: messages array with role + content
    messages = example["messages"]

    # Use tokenizer's chat template
    text = tokenizer.apply_chat_template(
        messages,
        tokenize=False,
        add_generation_prompt=False
    )

    return {"text": text}


def train(config: Dict[str, Any], model, tokenizer, dataset):
    """Execute LoRA training."""
    print(f"\n🎯 Starting training...")
    print(f"   Examples: {len(dataset)}")
    print(f"   Epochs: {config['epochs']}")
    print(f"   Batch size: {config['batchSize']}")
    print(f"   Learning rate: {config['learningRate']}")

    # Format dataset with chat template
    dataset = dataset.map(
        lambda example: format_chat_template(example, tokenizer),
        remove_columns=dataset.column_names
    )

    # Training arguments
    output_dir = config['outputDir']
    training_args = TrainingArguments(
        per_device_train_batch_size=config['batchSize'],
        gradient_accumulation_steps=4,  # Effective batch size = 4 * batch_size
        warmup_steps=5,
        num_train_epochs=config['epochs'],
        learning_rate=config['learningRate'],
        fp16=not is_bfloat16_supported(),
        bf16=is_bfloat16_supported(),
        logging_steps=1,
        optim="adamw_8bit",  # 8-bit Adam optimizer (less memory)
        weight_decay=0.01,
        lr_scheduler_type="linear",
        seed=42,
        output_dir=output_dir,
        report_to="none",  # Disable wandb/tensorboard for MVP
        save_strategy="epoch",
        save_total_limit=1,  # Only keep latest checkpoint
    )

    # SFT Trainer (Supervised Fine-Tuning)
    trainer = SFTTrainer(
        model=model,
        tokenizer=tokenizer,
        train_dataset=dataset,
        dataset_text_field="text",
        max_seq_length=2048,
        dataset_num_proc=2,
        packing=False,  # Don't pack multiple examples (simpler)
        args=training_args,
    )

    # Train!
    print(f"🚀 Training started (this will take a few minutes)...")
    trainer_stats = trainer.train()

    print(f"✅ Training complete!")
    print(f"   Final loss: {trainer_stats.training_loss:.4f}")
    print(f"   Training time: {trainer_stats.metrics['train_runtime']:.2f}s")
    print(f"   Examples/second: {trainer_stats.metrics['train_samples_per_second']:.2f}")

    return trainer


def save_adapter(model, tokenizer, output_dir: str):
    """Save trained LoRA adapter."""
    print(f"\n💾 Saving adapter to: {output_dir}")

    # Save LoRA adapter weights
    model.save_pretrained(output_dir)
    tokenizer.save_pretrained(output_dir)

    print(f"✅ Adapter saved successfully")
    print(f"   Files: adapter_config.json, adapter_model.safetensors")


def export_to_gguf(model, tokenizer, output_dir: str, base_model: str):
    """Export trained model to GGUF format for Ollama."""
    print(f"\n📦 Exporting to GGUF format...")

    gguf_path = os.path.join(output_dir, "model.gguf")

    try:
        # Unsloth provides built-in GGUF export
        model.save_pretrained_gguf(
            output_dir,
            tokenizer,
            quantization_method="q4_k_m"  # 4-bit quantization (balanced quality/size)
        )

        print(f"✅ GGUF export successful")
        print(f"   Output: {gguf_path}")
        print(f"   Quantization: q4_k_m (4-bit, medium quality)")

    except Exception as e:
        print(f"⚠️  GGUF export failed: {e}")
        print(f"   Fallback: Adapter saved in Hugging Face format")
        print(f"   You can convert manually using llama.cpp tools")


def main():
    """Main training pipeline."""
    parser = argparse.ArgumentParser(description="Unsloth LoRA Training")
    parser.add_argument("--config", required=True, help="Path to config JSON")
    parser.add_argument("--output", required=True, help="Output directory for adapter")

    args = parser.parse_args()

    print("🧬 Unsloth LoRA Training Script")
    print("=" * 60)

    # Step 1: Load configuration
    config = load_config(args.config)
    config['outputDir'] = args.output

    # Step 2: Load base model with Unsloth + LoRA
    model, tokenizer = load_model(
        config['baseModel'],
        config['rank'],
        config['alpha']
    )

    # Step 3: Load training dataset
    dataset = load_dataset_from_jsonl(config['datasetPath'])

    # Step 4: Train LoRA adapter
    trainer = train(config, model, tokenizer, dataset)

    # Step 5: Save adapter weights
    save_adapter(model, tokenizer, args.output)

    # Step 6: Export to GGUF (optional, for Ollama)
    export_to_gguf(model, tokenizer, args.output, config['baseModel'])

    print("\n" + "=" * 60)
    print("✅ Unsloth LoRA Training: SUCCESS")
    print(f"   Adapter saved to: {args.output}")
    print(f"   Ready for inference with Ollama or Hugging Face Transformers")


if __name__ == "__main__":
    main()
