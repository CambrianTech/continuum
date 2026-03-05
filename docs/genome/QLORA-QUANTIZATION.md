# QLoRA Quantization: Architecture & Conversion

QLoRA (4-bit quantized base model + full-precision LoRA weights) is the **primary training mode** in continuum's genome system. This document covers the quantization pipeline, format conversion, and mixed-precision inference.

## Why QLoRA is Primary

| Platform | VRAM | Max Model (Full Precision) | Max Model (QLoRA 4-bit) |
|----------|------|---------------------------|------------------------|
| M1 MacBook 16GB | 16GB unified | ~1B | **3B** |
| M1 MacBook 32GB | 32GB unified | ~3B | **8B** |
| RTX 5090 32GB | 32GB dedicated | ~8B | **14B+** |

QLoRA makes training accessible on consumer hardware. Without it, a 3B model requires ~12GB for weights + optimizer states. With 4-bit NF4 quantization, the base model shrinks to ~2GB, leaving room for LoRA weights and optimizer.

## The Key Insight: Adapters Are Always FP16

**LoRA adapters from QLoRA training are FP16/BF16 safetensors — format-identical to full-precision LoRA.**

The quantization only affects the base model during training. The LoRA weight matrices (A and B) are always trained and stored in full precision. This means:

- The same adapter works on both quantized and non-quantized base models
- No conversion needed for the adapter itself
- The conversion question is about the **base model**, not the adapter

## Training Configuration

QLoRA is the default in `GenomeTrainTypes.ts`:

```typescript
quantize: data.quantize ?? true,      // Default: enabled
quantizeBits: data.quantizeBits ?? 4, // Default: 4-bit NF4
```

### BitsAndBytes Configuration

The training script (`peft-train.py`) configures BitsAndBytes NF4:

```python
bnb_config = BitsAndBytesConfig(
    load_in_4bit=True,
    bnb_4bit_quant_type="nf4",        # Normal Float 4-bit
    bnb_4bit_use_double_quant=True,    # Double quantization (saves ~0.4GB on 3B model)
    bnb_4bit_compute_dtype=torch.bfloat16  # Compute in BF16 for stability
)
```

### Fallback Chain

When 4-bit quantization fails (e.g., BitsAndBytes not available on MPS):

```
4-bit NF4 → 8-bit INT8 → Full Precision (FP16)
```

The actual quantization used is recorded in `quantization_info.json` alongside the adapter.

## Metadata Pipeline

Quantization metadata flows through the entire pipeline:

```
peft-train.py                    # Writes quantization_info.json
    ↓
BaseServerLoRATrainer            # Reads quantization_info.json into AdapterManifest
    ↓
AdapterPackage.buildManifest()   # Includes QuantizationInfo in manifest
    ↓
GenomeLayerEntity                # Persists quantization field in database
    ↓
GenomeConvertPipeline            # Uses metadata to determine conversion direction
```

### QuantizationInfo Type

```typescript
export interface QuantizationInfo {
  enabled: boolean;              // Was base model quantized during training?
  bits: 4 | 8;                  // Quantization precision
  type: 'nf4' | 'int4' | 'int8' | 'fp8';  // Quantization algorithm
  doubleQuant: boolean;          // BitsAndBytes double quantization
  computeDtype: 'bfloat16' | 'float16';    // Compute precision
}
```

## Conversion Matrix

| From | To | Operation | Use Case |
|------|----|-----------|----------|
| LoRA + FP16 base | Merged FP16 safetensors | `merge-full` | Deploy to 5090 (no adapter overhead) |
| LoRA + FP16 base | Merged GGUF Q4/Q8 | `merge-and-quantize` | Deploy to M1 (single file, quantized) |
| HuggingFace model | GGUF Q4/Q8 | `quantize-base` | Quantize base model for local inference |
| QLoRA adapter | Same adapter on FP16 base | (no conversion needed) | Adapters are already FP16 |
| Converted model | Validation report | `validate` | Sanity-check converted output |

### genome/convert Command

```bash
# Merge LoRA into full-precision model
./jtag genome/convert --operation=merge-full \
  --adapterPath=.continuum/genome/adapters/helper-coding-123 \
  --baseModel=unsloth/Llama-3.2-3B-Instruct

# Merge LoRA and quantize to 4-bit GGUF
./jtag genome/convert --operation=merge-and-quantize \
  --adapterPath=.continuum/genome/adapters/helper-coding-123 \
  --baseModel=unsloth/Llama-3.2-3B-Instruct \
  --bits=4

# Quantize base model to GGUF (no LoRA)
./jtag genome/convert --operation=quantize-base \
  --baseModel=unsloth/Llama-3.2-3B-Instruct \
  --bits=4

# Validate a converted model
./jtag genome/convert --operation=validate \
  --adapterPath=.continuum/genome/converted/model-q4-1234567890
```

Conversions run through the Rust sentinel for process isolation, timeout enforcement (30 minutes), and log capture.

## Candle Inference: Mixed-Precision LoRA on GGUF

The Rust inference engine (Candle) supports applying FP16 LoRA adapters to GGUF-quantized base models.

### How It Works

For each layer with a LoRA adapter:

1. **Dequantize** the base weight from QTensor to FP32
2. **Apply LoRA**: `W' = W + scale * (B × A)`
3. **Store** as `QMatMul::Tensor(merged)` (non-quantized)

Only layers with LoRA adapters are dequantized. All other layers remain in their original quantized format.

```rust
fn merge_lora(&mut self, lora_a: &Tensor, lora_b: &Tensor, scale: f64, device: &Device) -> Result<()> {
    let base_weight = match &self.inner {
        QMatMul::QTensor(qt) => qt.dequantize(device)?,     // Quantized → FP32
        QMatMul::Tensor(t) => t.clone(),                     // Already FP32
        QMatMul::TensorF16(t) => t.to_dtype(DType::F32)?,   // FP16 → FP32
    };
    let delta = lora_b.matmul(lora_a)?;                     // B × A
    let scaled_delta = (delta * scale)?;                     // α/r scaling
    let merged = (&base_weight + &scaled_delta)?;            // W + ΔW
    self.inner = QMatMul::Tensor(merged);                    // Store unquantized
    Ok(())
}
```

### LoRA Name Mapping

PEFT uses a different naming convention than GGUF. The inference engine parses LoRA names to extract (layer_index, projection_type) and maps directly to `LayerWeights` struct fields:

| PEFT Name | GGUF Field |
|-----------|------------|
| `model.layers.N.self_attn.q_proj` | `layers[N].attention_wq` |
| `model.layers.N.self_attn.k_proj` | `layers[N].attention_wk` |
| `model.layers.N.self_attn.v_proj` | `layers[N].attention_wv` |
| `model.layers.N.self_attn.o_proj` | `layers[N].attention_wo` |
| `model.layers.N.mlp.gate_proj` | `layers[N].feed_forward_w1` |
| `model.layers.N.mlp.up_proj` | `layers[N].feed_forward_w3` |
| `model.layers.N.mlp.down_proj` | `layers[N].feed_forward_w2` |

All 7 projections per layer are supported — 196 LoRA layers total on Llama-3.2-3B.

### Memory Impact

Dequantizing a QTensor from Q4_0 to FP32 increases its memory footprint by ~8x. For a 3B model with 196 LoRA layers across 28 transformer blocks:

- **Base GGUF Q4**: ~1.8GB
- **After LoRA merge** (worst case, all layers): ~5.5GB peak during merge, then ~5.5GB steady state
- **Selective merge** (attention only, 112 layers): ~3.5GB steady state

The `GpuMemoryManager` should be consulted before merge operations to ensure sufficient headroom.

## Sentinel Conversion Pipeline

Automated conversion workflows use the `GenomeConvertPipeline`:

```typescript
import { buildGenomeConvertPipeline } from '@system/sentinel/pipelines/GenomeConvertPipeline';

const pipeline = buildGenomeConvertPipeline({
  operation: 'merge-and-quantize',
  adapterPath: '.continuum/genome/adapters/helper-coding-123',
  baseModel: 'unsloth/Llama-3.2-3B-Instruct',
  bits: 4,
  personaId: helperPersonaId,
  domain: 'coding',
});

await Commands.execute('sentinel/run', { definition: pipeline });
```

### Academy Integration

The Academy dual-sentinel can chain training with conversion:

```typescript
import { buildTrainAndConvertPipeline } from '@system/sentinel/pipelines/GenomeConvertPipeline';
import { buildLoRATrainingPipeline } from '@system/sentinel/pipelines/LoRATrainingPipeline';

const trainingPipeline = buildLoRATrainingPipeline({
  personaId, personaName, roomId,
  traitType: 'coding',
});

const fullPipeline = buildTrainAndConvertPipeline(trainingPipeline, {
  baseModel: 'unsloth/Llama-3.2-3B-Instruct',
  bits: 4,
  personaId,
  domain: 'coding',
});
```

This produces:
1. QLoRA-trained adapter (FP16 safetensors) — registered as primary
2. GGUF-merged variant (quantized single file) — registered as deployment variant

Both versions coexist in the GenomeRegistry with different format tags.

## Performance Targets

| Metric | M1 16GB | RTX 5090 |
|--------|---------|----------|
| Training (QLoRA 4-bit, 3B, 20 examples) | ~25s | ~8s |
| Merge-full (LoRA → FP16 merged) | ~60s | ~20s |
| Merge-and-quantize (LoRA → GGUF Q4) | ~120s | ~40s |
| Inference (GGUF Q4 + LoRA) | ~50 tok/s | ~200 tok/s |
| Inference (FP16 merged) | N/A (OOM) | ~150 tok/s |

## Verification

```bash
# 1. Train with QLoRA (default mode)
./jtag genome/train --personaId=HELPER --domain=coding

# 2. Verify manifest has quantization metadata
cat .continuum/genome/adapters/*/manifest.json | jq .quantization

# 3. Convert to GGUF-merged
./jtag genome/convert --operation=merge-and-quantize \
  --adapterPath=.continuum/genome/adapters/helper-coding-* \
  --baseModel=unsloth/Llama-3.2-3B-Instruct

# 4. Verify both formats in registry
./jtag data/list --collection=genome_layers --limit=10

# 5. Inference on GGUF base + FP16 LoRA (mixed-precision)
# Candle loads GGUF base, applies FP16 LoRA via dequantize→merge→store
```
