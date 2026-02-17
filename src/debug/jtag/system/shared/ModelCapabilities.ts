/**
 * ModelCapabilities — Fine-Tuning, Quantization, and Adapter Type System
 * ======================================================================
 *
 * Defines the complete capability profile of a model on a specific provider/runtime.
 * This is the "knowing" layer — every adapter reports what it supports, and
 * algorithms query across all registered models to find the best horsepower
 * for the desired outcome.
 *
 * Design principle: Define everything now, populate incrementally.
 * Adapters report what they know at discovery time. Unknown fields stay undefined.
 * The type system is the source of truth for what CAN be known.
 *
 * Usage:
 *   // At adapter discovery time:
 *   registry.register({
 *     modelId: 'meta-llama/Llama-3.1-8B-Instruct',
 *     provider: 'candle',
 *     contextWindow: 1400,
 *     capabilities: { ... },
 *     adapterProfile: {
 *       quantization: { format: QuantFormat.Q4_K_M, bitsPerWeight: 4 },
 *       fineTuning: { supportedMethods: [AdapterMethod.QLORA] },
 *       runtime: InferenceRuntime.CANDLE,
 *       ...
 *     }
 *   });
 *
 *   // At selection time:
 *   const candidates = registry.getAll('meta-llama/Llama-3.1-8B-Instruct')
 *     .filter(m => m.adapterProfile?.fineTuning.supportedMethods.includes(AdapterMethod.QLORA))
 *     .filter(m => (m.adapterProfile?.hardware.inferenceVramMB ?? Infinity) <= availableVram);
 */

// ═══════════════════════════════════════════════════════════════════════
// QUANTIZATION
// ═══════════════════════════════════════════════════════════════════════

/**
 * Weight quantization formats.
 *
 * Ordered roughly by quality (highest first, most compressed last).
 * Each format represents a tradeoff between model quality, inference speed,
 * and memory usage. The choice of quantization format directly affects
 * whether LoRA adapters can be injected and how.
 */
export enum QuantFormat {
  /** Full 32-bit floating point — maximum quality, 4x memory */
  FP32 = 'fp32',

  /** Brain floating point 16 — good quality, native on modern GPUs */
  BF16 = 'bf16',

  /** IEEE 16-bit float — good quality, widely supported */
  FP16 = 'fp16',

  /** 8-bit integer — good balance, 2x compression from FP16 */
  INT8 = 'int8',

  /** GGML 8-bit — llama.cpp native, good quality */
  Q8_0 = 'q8_0',

  /** GGML 6-bit with K-quant — very good quality/size ratio */
  Q6_K = 'q6_k',

  /** GGML 5-bit medium K-quant — solid quality, moderate compression */
  Q5_K_M = 'q5_k_m',

  /** GGML 4-bit medium K-quant — the sweet spot for most local inference */
  Q4_K_M = 'q4_k_m',

  /** GGML 4-bit basic — faster than K_M, slightly lower quality */
  Q4_0 = 'q4_0',

  /** GGML 3-bit medium K-quant — aggressive compression, quality loss */
  Q3_K_M = 'q3_k_m',

  /** GGML 2-bit K-quant — maximum compression, significant quality loss */
  Q2_K = 'q2_k',

  /** GPTQ — GPU-optimized post-training quantization */
  GPTQ = 'gptq',

  /** AWQ — activation-aware weight quantization */
  AWQ = 'awq',

  /** No quantization applied (full precision weights) */
  NONE = 'none',
}

/**
 * Model weight container/file formats.
 * Distinct from quantization — a GGUF file can contain Q4_K_M or Q8_0 weights.
 */
export enum WeightFormat {
  /** GGUF — llama.cpp/Candle native, self-describing, versioned */
  GGUF = 'gguf',

  /** SafeTensors — HuggingFace standard, memory-mapped, safe */
  SAFETENSORS = 'safetensors',

  /** PyTorch checkpoint — legacy, widely supported */
  PYTORCH = 'pytorch',

  /** ONNX — cross-platform inference */
  ONNX = 'onnx',

  /** MLX — Apple Silicon native format */
  MLX = 'mlx',

  /** Cloud API — no local weights, opaque */
  CLOUD = 'cloud',
}

/**
 * Full quantization profile for a model instance.
 */
export interface QuantizationProfile {
  /** Quantization method applied to weights */
  readonly format: QuantFormat;

  /** Bits per weight (4, 8, 16, 32) */
  readonly bitsPerWeight: number;

  /** Container format for the weights */
  readonly weightFormat?: WeightFormat;

  /** Can weights be dequantized to higher precision for training? */
  readonly canDequantizeForTraining?: boolean;

  /** Can adapters be trained directly on quantized weights? (QLoRA = true) */
  readonly canTrainInQuantized?: boolean;

  /** Group size for quantization (e.g., 128 for GPTQ) — affects quality */
  readonly groupSize?: number;
}


// ═══════════════════════════════════════════════════════════════════════
// FINE-TUNING / ADAPTER METHODS
// ═══════════════════════════════════════════════════════════════════════

/**
 * Parameter-efficient fine-tuning (PEFT) methods.
 *
 * These are the techniques for injecting learned behavior into a base model
 * without modifying the base weights. Each method has different tradeoffs
 * in quality, speed, memory, and composability.
 *
 * For the genome paging vision: each "genomic trait" is an adapter trained
 * with one of these methods. The runtime loads/unloads them as needed.
 */
export enum AdapterMethod {
  /** Low-Rank Adaptation — adds trainable rank decomposition matrices */
  LORA = 'lora',

  /** Quantized LoRA — LoRA on quantized base model (4-bit base + FP16 adapters) */
  QLORA = 'qlora',

  /** Weight-Decomposed Low-Rank Adaptation — improved LoRA with magnitude component */
  DORA = 'dora',

  /** Full fine-tuning — modifies all weights (highest quality, highest cost) */
  FULL = 'full',

  /** Prefix tuning — prepends trainable tokens to each layer */
  PREFIX = 'prefix',

  /** Prompt tuning — soft prompts prepended to input only */
  PROMPT = 'prompt',

  /** Infused Adapter by Inhibiting and Amplifying Inner Activations */
  IA3 = 'ia3',

  /** Adapter layers — bottleneck modules inserted between transformer layers */
  ADAPTER_LAYERS = 'adapter_layers',
}

/**
 * Transformer layers that can be targeted by adapters.
 *
 * Different adapter methods target different layers. LoRA typically targets
 * attention projections (Q, K, V, O). Some configurations also target MLP layers
 * for better quality at higher parameter cost.
 */
export enum AdapterTarget {
  /** Attention query projection */
  ATTN_Q = 'attn_q',

  /** Attention key projection */
  ATTN_K = 'attn_k',

  /** Attention value projection */
  ATTN_V = 'attn_v',

  /** Attention output projection */
  ATTN_O = 'attn_o',

  /** MLP gate projection (SwiGLU architectures) */
  MLP_GATE = 'mlp_gate',

  /** MLP up projection */
  MLP_UP = 'mlp_up',

  /** MLP down projection */
  MLP_DOWN = 'mlp_down',

  /** Token embedding layer */
  EMBEDDING = 'embedding',

  /** Language model head (output projection) */
  LM_HEAD = 'lm_head',
}

/**
 * LoRA-specific configuration and constraints.
 */
export interface LoRAProfile {
  /** Maximum supported rank (higher = more parameters = better quality = more VRAM) */
  readonly maxRank: number;

  /** Recommended rank for this model/hardware combo */
  readonly recommendedRank: number;

  /** Typical alpha value (scaling factor, usually = rank or 2*rank) */
  readonly recommendedAlpha?: number;

  /** Maximum number of adapters that can be loaded simultaneously */
  readonly maxConcurrentAdapters: number;

  /** Can multiple adapters be composed/stacked at inference time? */
  readonly supportsStacking: boolean;

  /** Estimated adapter size in MB at recommended rank */
  readonly adapterSizeMB: number;

  /** Which layers can be targeted */
  readonly targetableLayers: readonly AdapterTarget[];

  /** Recommended target layers for best quality/cost ratio */
  readonly recommendedTargets?: readonly AdapterTarget[];

  /** Dropout rate recommended for training (0.0 - 1.0) */
  readonly recommendedDropout?: number;
}

/**
 * Fine-tuning capability profile.
 */
export interface FineTuningProfile {
  /** Which adapter methods this model supports on this runtime */
  readonly supportedMethods: readonly AdapterMethod[];

  /** LoRA-specific parameters (present if LORA or QLORA is supported) */
  readonly lora?: LoRAProfile;

  /** Maximum training batch size on this hardware */
  readonly maxTrainingBatchSize?: number;

  /** Whether gradient checkpointing is supported (saves VRAM at speed cost) */
  readonly supportsGradientCheckpointing?: boolean;

  /** Whether flash attention is available (faster training) */
  readonly supportsFlashAttention?: boolean;
}


// ═══════════════════════════════════════════════════════════════════════
// INFERENCE RUNTIME
// ═══════════════════════════════════════════════════════════════════════

/**
 * Local inference runtimes.
 * Each runtime has different capabilities for loading models and adapters.
 */
export enum InferenceRuntime {
  /** Candle — Rust-native, GGUF/SafeTensors, Metal acceleration */
  CANDLE = 'candle',

  /** llama.cpp — C++, GGUF, Metal/CUDA/CPU, mature ecosystem */
  LLAMA_CPP = 'llama_cpp',

  /** MLX — Apple Silicon native, Python, excellent Metal performance */
  MLX = 'mlx',

  /** ONNX Runtime — cross-platform, optimized inference graphs */
  ONNX = 'onnx',

  /** HuggingFace Transformers — Python, full ecosystem, all formats */
  TRANSFORMERS = 'transformers',

  /** vLLM — high-throughput serving, PagedAttention, CUDA */
  VLLM = 'vllm',

  /** Text Generation Inference — HuggingFace serving, optimized */
  TGI = 'tgi',

  /** Ollama — wrapper around llama.cpp with model management */
  OLLAMA = 'ollama',

  /** Cloud API — opaque, no local execution */
  CLOUD_API = 'cloud_api',
}

/**
 * Hardware accelerator type.
 */
export enum Accelerator {
  /** Apple Metal Performance Shaders */
  METAL = 'metal',

  /** NVIDIA CUDA */
  CUDA = 'cuda',

  /** AMD ROCm */
  ROCM = 'rocm',

  /** Intel oneAPI */
  ONEAPI = 'oneapi',

  /** CPU only (no GPU acceleration) */
  CPU = 'cpu',

  /** Cloud-managed (unknown hardware) */
  CLOUD = 'cloud',
}


// ═══════════════════════════════════════════════════════════════════════
// HARDWARE PROFILE
// ═══════════════════════════════════════════════════════════════════════

/**
 * Hardware requirements and measured performance for a model on specific hardware.
 */
export interface HardwareProfile {
  /** VRAM required for inference (MB) */
  readonly inferenceVramMB: number;

  /** VRAM required for fine-tuning with recommended method (MB) */
  readonly trainingVramMB?: number;

  /** Accelerator type used */
  readonly accelerator: Accelerator;

  /** Measured tokens per second on THIS hardware (inference, prompt eval) */
  readonly measuredInputTPS?: number;

  /** Measured tokens per second on THIS hardware (inference, generation) */
  readonly measuredOutputTPS?: number;

  /** Whether the model fits entirely in VRAM (vs. partial offload to RAM) */
  readonly fitsInVram?: boolean;

  /** Number of layers offloaded to CPU (0 = fully GPU) */
  readonly cpuOffloadLayers?: number;
}


// ═══════════════════════════════════════════════════════════════════════
// COMPOSITE: MODEL ADAPTER PROFILE
// ═══════════════════════════════════════════════════════════════════════

/**
 * Complete adapter/fine-tuning profile for a model on a specific provider.
 *
 * This is the top-level type that gets attached to ModelMetadata.
 * It captures everything we need to know to make algorithmic decisions
 * about model selection, adapter loading, and training scheduling.
 *
 * Example — Llama 3.1 8B on Candle/M1:
 *   {
 *     runtime: InferenceRuntime.CANDLE,
 *     quantization: { format: QuantFormat.Q4_K_M, bitsPerWeight: 4, weightFormat: WeightFormat.GGUF },
 *     fineTuning: {
 *       supportedMethods: [AdapterMethod.QLORA],
 *       lora: {
 *         maxRank: 32,
 *         recommendedRank: 8,
 *         maxConcurrentAdapters: 3,
 *         supportsStacking: true,
 *         adapterSizeMB: 15,
 *         targetableLayers: [AdapterTarget.ATTN_Q, AdapterTarget.ATTN_V],
 *       }
 *     },
 *     hardware: {
 *       inferenceVramMB: 4500,
 *       trainingVramMB: 8000,
 *       accelerator: Accelerator.METAL,
 *       measuredInputTPS: 40,
 *     }
 *   }
 */
export interface ModelAdapterProfile {
  /** Inference runtime this profile describes */
  readonly runtime: InferenceRuntime;

  /** Quantization details */
  readonly quantization: QuantizationProfile;

  /** Fine-tuning capabilities */
  readonly fineTuning: FineTuningProfile;

  /** Hardware requirements and measured performance */
  readonly hardware?: HardwareProfile;

  /** Model architecture family (for adapter compatibility checking) */
  readonly architectureFamily?: string;

  /** Parameter count in billions (e.g., 7, 8, 13, 70) */
  readonly parameterCountB?: number;

  /** Number of transformer layers (for adapter targeting) */
  readonly layerCount?: number;

  /** Hidden dimension size (affects adapter parameter count) */
  readonly hiddenSize?: number;
}


// ═══════════════════════════════════════════════════════════════════════
// QUERY HELPERS
// ═══════════════════════════════════════════════════════════════════════

/**
 * Check if a model supports any form of adapter-based fine-tuning.
 */
export function isFineTunable(profile: ModelAdapterProfile | undefined): boolean {
  if (!profile) return false;
  return profile.fineTuning.supportedMethods.length > 0
    && !profile.fineTuning.supportedMethods.every(m => m === AdapterMethod.FULL);
}

/**
 * Check if a model supports LoRA or QLoRA (the primary genome methods).
 */
export function supportsLoRA(profile: ModelAdapterProfile | undefined): boolean {
  if (!profile) return false;
  return profile.fineTuning.supportedMethods.includes(AdapterMethod.LORA)
    || profile.fineTuning.supportedMethods.includes(AdapterMethod.QLORA);
}

/**
 * Check if a model can stack multiple adapters simultaneously.
 * Required for genome paging (multiple traits loaded at once).
 */
export function supportsAdapterStacking(profile: ModelAdapterProfile | undefined): boolean {
  if (!profile?.fineTuning.lora) return false;
  return profile.fineTuning.lora.supportsStacking
    && profile.fineTuning.lora.maxConcurrentAdapters > 1;
}

/**
 * Estimate VRAM required for a LoRA adapter at a given rank.
 * Rough formula: 2 * rank * hiddenSize * targetLayers * 2 (bytes for FP16) / 1MB
 */
export function estimateAdapterVramMB(
  profile: ModelAdapterProfile,
  rank?: number
): number {
  const r = rank ?? profile.fineTuning.lora?.recommendedRank ?? 8;
  const hidden = profile.hiddenSize ?? 4096;
  const layers = profile.fineTuning.lora?.targetableLayers.length ?? 2;
  const transformerLayers = profile.layerCount ?? 32;
  // Each LoRA adapter adds two matrices per target per layer: A (hidden x rank) + B (rank x hidden)
  const bytesPerAdapter = 2 * r * hidden * layers * transformerLayers * 2; // FP16 = 2 bytes
  return Math.ceil(bytesPerAdapter / (1024 * 1024));
}

/**
 * Check if a model can run on given available VRAM.
 */
export function fitsInVram(
  profile: ModelAdapterProfile | undefined,
  availableVramMB: number
): boolean {
  if (!profile?.hardware) return false;
  return profile.hardware.inferenceVramMB <= availableVramMB;
}
