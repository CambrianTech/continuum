# Unsloth LoRA Training Setup

This document explains how to set up the Python environment for Unsloth-based LoRA fine-tuning.

## Prerequisites

- Python 3.8 or higher
- CUDA-capable GPU (for actual training, optional for testing)
- At least 8GB GPU VRAM recommended

## Installation

### 1. Install Python Dependencies

```bash
# Install unsloth with CUDA support (for GPU training)
pip install unsloth[cu118]  # For CUDA 11.8
# OR
pip install unsloth[cu121]  # For CUDA 12.1

# Install additional required packages
pip install transformers datasets accelerate
```

### 2. Verify Installation

```bash
python3 -c "import unsloth; import transformers; import datasets; print('✅ All dependencies installed')"
```

## Testing

### Phase 1: Dry Run (No Dependencies Required)

```bash
cd src/debug/jtag

# Test cost/time estimates without actual training
./jtag genome/train \
  --personaId="5a8316a5-e00c-441c-8601-15e61136f834" \
  --provider="unsloth" \
  --datasetPath="test-dataset-typescript.jsonl" \
  --dryRun=true
```

Expected output:
```
📁 GENOME TRAIN: Loading dataset from file
✅ Dataset loaded: 5 training examples
💰 Cost estimate: $0.0000
⏱️  Time estimate: 0.4s
🔍 DRY RUN: Skipping actual training
```

### Phase 2: Actual Training (Requires Dependencies)

```bash
# Train LoRA adapter on test dataset
./jtag genome/train \
  --personaId="5a8316a5-e00c-441c-8601-15e61136f834" \
  --provider="unsloth" \
  --datasetPath="test-dataset-typescript.jsonl" \
  --baseModel="llama3.2:3b" \
  --epochs=1 \
  --rank=8 \
  --dryRun=false
```

Expected output:
```
🧬 Starting Unsloth LoRA training...
   Model: llama3.2:3b
   Examples: 5
   Epochs: 1
   Executing: python3 .../unsloth-train.py

[Training progress logs]

✅ Training complete in 5.23s
   Adapter saved to: .continuum/genome/adapters/Helper-AI-conversational-1234567890
```

## Troubleshooting

### Error: "ModuleNotFoundError: No module named 'unsloth'"

**Problem**: Python dependencies not installed

**Solution**: Follow installation steps above

### Error: "CUDA out of memory"

**Problem**: GPU doesn't have enough VRAM for the base model

**Solutions**:
1. Use a smaller base model (e.g., `llama3.2:1b` instead of `llama3.2:3b`)
2. Reduce batch size: `--batchSize=1`
3. Reduce LoRA rank: `--rank=4`
4. Use CPU training (slower): Set `CUDA_VISIBLE_DEVICES=` to disable GPU

### Error: "Training script failed with exit code 1"

**Problem**: Python script encountered an error

**Solution**: Check the stderr output in the error message for details

## System Architecture

**Command Flow**:
```
./jtag genome/train
  → GenomeTrainServerCommand.execute()
    → UnslothLoRAAdapter.trainLoRA()
      → spawn('python3', ['unsloth-train.py', ...])
        → Python script trains LoRA adapter
        → Saves to /tmp/jtag-training-*/
      → Copy adapter to .continuum/genome/adapters/
    → Return modelPath, metrics
```

**Key Files**:
- `commands/genome/train/` - CLI command
- `system/genome/fine-tuning/server/adapters/UnslothLoRAAdapter.ts` - TypeScript adapter
- `system/genome/fine-tuning/server/adapters/scripts/unsloth-train.py` - Python training script

## Next Steps

After successful training:

1. **Verify Adapter Files**: Check `.continuum/genome/adapters/` for saved adapter
2. **Load in Ollama**: Use `ollama create` to load the adapter as a custom model
3. **Test Inference**: Chat with the fine-tuned model to verify learning
4. **Wire to PersonaUser**: Connect TrainingDataAccumulator auto-training to this pipeline

See `TESTING-GENOME-TRAINING.md` for the complete testing roadmap.
