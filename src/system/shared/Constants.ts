/**
 * System Constants - SINGLE SOURCE OF TRUTH
 *
 * ⚠️ CRITICAL: ALL system constants MUST be defined here
 * ⚠️ NEVER hardcode paths, values, or magic strings anywhere else
 * ⚠️ Import from this file ONLY - no duplication allowed
 *
 * This file exists because scattered constants create technical debt.
 * Every hardcoded value is a future bug waiting to happen.
 */

/**
 * File System Paths - Browser-relative paths ONLY
 *
 * ⚠️ DO NOT add database paths or other server-only paths here!
 * ⚠️ This file is bundled into browser code
 * ⚠️ Server code MUST use SystemPaths.* (from system/core/config/SystemPaths)
 * ⚠️ Use DatabaseConfig.ts for server-only database paths
 */
export const PATHS = {
  /** Base continuum directory */
  CONTINUUM: '.continuum',

  /** Training datasets directory - users can override with DATASETS_DIR env var for external drives */
  DATASETS: '.continuum/datasets',
  DATASETS_PARSED: '.continuum/datasets/parsed',
  DATASETS_PREPARED: '.continuum/datasets/prepared',

  /** Media processing output */
  MEDIA_OUTPUT: '.continuum/media',
  MEDIA_TEMP: '.continuum/media/temp'
} as const;

/**
 * Environment Variables - standard names we check
 */
export const ENV_VARS = {
  DATABASE_DIR: 'DATABASE_DIR',
  DATABASE_BACKUP_DIR: 'DATABASE_BACKUP_DIR',
  DATABASE_ARCHIVE_DIR: 'DATABASE_ARCHIVE_DIR',
  DATASETS_DIR: 'DATASETS_DIR',
  REPO_PATH: 'REPO_PATH',
  SENTINEL_PATH: 'SENTINEL_PATH'
} as const;

/**
 * Collection Names - Browser-safe, used for API calls
 *
 * ⚠️ AUTO-GENERATED via generator/generate-collection-constants.ts
 * ⚠️ Re-exported from shared/generated-collection-constants.ts
 * ⚠️ NEVER hardcode collection strings - use COLLECTIONS.* constants
 *
 * Source of truth: Entity files with `static readonly collection`
 * Run: npx tsx generator/generate-collection-constants.ts
 */
export { COLLECTIONS, type CollectionName } from '../../shared/generated-collection-constants';


/**
 * Fine-Tuning Providers - Supported providers for LoRA training
 */
export const FINE_TUNING_PROVIDERS = [
  'deepseek',
  'fireworks',
  'mistral',
  'openai',
  'together'
] as const;

export type FineTuningProvider = typeof FINE_TUNING_PROVIDERS[number];

/**
 * Model IDs - SINGLE SOURCE OF TRUTH for AI model identifiers
 *
 * ⚠️ CRITICAL: ALL model IDs must be defined here
 * ⚠️ NEVER hardcode model version strings anywhere else
 * ⚠️ When a provider updates models, change it ONCE here
 *
 * Why this exists: Model IDs were scattered across 5+ files.
 * Anthropic updates meant hunting through the entire codebase.
 * This ensures ONE change updates EVERYWHERE.
 */
export const MODEL_IDS = {
  /** Anthropic Claude models */
  ANTHROPIC: {
    SONNET_4_5: 'claude-sonnet-4-5-20250929',       // Current Claude Sonnet 4.5 (Sep 2025)
    OPUS_4: 'claude-opus-4-20250514',               // Claude Opus 4 (May 2025)
    HAIKU_4_5: 'claude-haiku-4-5-20251001',         // Claude Haiku 4.5 (Oct 2025)
    // Legacy alias — consumers should migrate to HAIKU_4_5
    HAIKU_3_5: 'claude-haiku-4-5-20251001'          // Points to Haiku 4.5 now
  },

  /** OpenAI models */
  OPENAI: {
    GPT_4: 'gpt-4',
    GPT_4_TURBO: 'gpt-4-turbo-preview',
    GPT_3_5_TURBO: 'gpt-3.5-turbo'
  },

  /** DeepSeek models */
  DEEPSEEK: {
    CHAT: 'deepseek-chat',
    CODER: 'deepseek-coder'
  },

  /** Groq models */
  GROQ: {
    LLAMA_3_1_8B: 'llama-3.1-8b-instant',
    LLAMA_3_1_70B: 'llama-3.1-70b-versatile'
  },

  /** Together.ai models */
  TOGETHER: {
    LLAMA_3_1_70B: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo',
    LLAMA_3_1_8B: 'meta-llama/Meta-Llama-3.1-8B-Instruct-Turbo'
  },

  /** Fireworks models */
  FIREWORKS: {
    DEEPSEEK_V3: 'accounts/fireworks/models/deepseek-v3',
    LLAMA_3_1_8B: 'accounts/fireworks/models/llama-v3p1-8b-instruct',
    LLAMA_3_1_70B: 'accounts/fireworks/models/llama-v3p1-70b-instruct',
    LLAMA_3_1_405B: 'accounts/fireworks/models/llama-v3p1-405b-instruct',
    MIXTRAL_8X7B: 'accounts/fireworks/models/mixtral-8x7b-instruct',
    QWEN_2_5_72B: 'accounts/fireworks/models/qwen2p5-72b-instruct'
  },

  /** xAI models */
  XAI: {
    GROK_3: 'grok-3',  // Updated from grok-beta (deprecated 2025-09-15)
    GROK_4: 'grok-4'
  },

  /** Historical local aliases. Do not use for Continuum runtime selection. */
  CANDLE: {
    QWEN_GATING: 'Qwen/Qwen2-0.5B-Instruct',
    QWEN_DEFAULT: 'continuum-ai/qwen3.5-4b-code-forged-GGUF'
  },

  /** Sentinel local models */
  SENTINEL: {
    GPT2: 'gpt2',
    DISTILGPT2: 'distilgpt2'
  }
} as const;

/**
 * LOCAL_MODELS - SINGLE SOURCE OF TRUTH for local inference
 *
 * ⚠️ CRITICAL: This is the canonical model configuration for native Rust inference
 * ⚠️ All model mappings, preloads, and defaults come from here
 * ⚠️ Local runtime/admission reads from here - DO NOT duplicate mappings elsewhere
 *
 * Local alpha models are Qwen: Qwen3.5 for text/code and Qwen2-VL for vision.
 * Runtime selection is Rust-owned so VRAM/unified-memory pressure, LoRA paging,
 * and future MoE/base-model paging stay under one scheduler.
 */
export const LOCAL_MODELS = {
  /** Default models for inference worker to preload at startup */
  PRELOAD: [
    'continuum-ai/qwen3.5-4b-code-forged-GGUF',  // Our forged Qwen3.5 — auto-selects best quant for hardware
    'Qwen/Qwen2-0.5B-Instruct',                   // Fast model for gating/classification
  ],

  /** Default model for local inference.
   *  Rust auto-selects Q4_K_M or Q8_0 based on available RAM.
   *  Our own forged model — 70%+ HumanEval, runs on 8GB devices. */
  DEFAULT: 'continuum-ai/qwen3.5-4b-code-forged-GGUF',

  /** Native-vision local model (Vision AI persona).
   *  Bound to qwen2-vl-7b-instruct via the in-process llamacpp adapter
   *  with mmproj. Single string lives here; personas.ts + models.toml +
   *  any future caller all read this constant so a model swap is one edit.
   *  See #963 for the eventual Rust↔TS shared source-of-truth. */
  VISION: 'qwen2-vl-7b-instruct',

  /** Fast model for gating/classification tasks */
  GATING: 'Qwen/Qwen2-0.5B-Instruct',

  /**
   * Coding agent model — Qwen2.5-Coder-14B compacted (GGUF Q5_K_S, 9GB).
   * Resolved server-side by model_registry.json in the Candle adapter.
   * On 32GB machines with bf16/ dir present, Rust auto-upgrades to BF16 batch prefill.
   */
  CODING_AGENT: 'coder',

  /** BF16 batch-prefill variant — explicitly selects the safetensors backend (32GB+ only) */
  CODING_AGENT_BF16: 'coder-bf16',

  /** Explicit local aliases accepted by local model adapters. */
  LEGACY_TO_HUGGINGFACE: {
    'qwen3.5': 'continuum-ai/qwen3.5-4b-code-forged-GGUF',
    'qwen3.5:4b': 'continuum-ai/qwen3.5-4b-code-forged-GGUF',
    'qwen3.5-code': 'continuum-ai/qwen3.5-4b-code-forged-GGUF',
    'qwen2-vl': 'qwen2-vl-7b-instruct',
    'qwen2:0.5b': 'Qwen/Qwen2-0.5B-Instruct',
    'qwen2': 'Qwen/Qwen2-0.5B-Instruct',

    'qwen2.5': 'Qwen/Qwen2.5-7B-Instruct',
  } as const,

  /**
   * Map a model name to HuggingFace ID
   * Returns original if not found (might already be a HuggingFace ID)
   */
  mapToHuggingFace(modelName: string): string {
    const normalized = modelName.toLowerCase().trim();
    const mapping = LOCAL_MODELS.LEGACY_TO_HUGGINGFACE as Record<string, string>;

    // Direct lookup
    if (mapping[normalized]) {
      return mapping[normalized];
    }

    // Try without version suffix (e.g., 'qwen3.5:4b-instruct' -> 'qwen3.5:4b')
    const withoutSuffix = normalized.replace(/-instruct.*$|-chat.*$|-q\d+.*$/i, '');
    if (mapping[withoutSuffix]) {
      return mapping[withoutSuffix];
    }

    // Not found - assume it's already a HuggingFace ID
    return modelName;
  }
} as const;


/**
 * TTS_MODELS - SINGLE SOURCE OF TRUTH for Text-to-Speech models
 *
 * ⚠️ CRITICAL: ALL TTS model metadata must be defined here
 * ⚠️ NEVER hardcode HF repos, sample rates, or model paths in individual adapters
 * ⚠️ Same pattern as LOCAL_MODELS — centralize, reference everywhere
 *
 * Each TTS adapter in Rust reads from this via IPC or build-time config.
 * Adding a new TTS model means ONE entry here, not scattered across adapter files.
 */
export const TTS_MODELS = {
  /** Edge-TTS — Microsoft's free neural TTS API (cloud, no key needed) */
  EDGE: {
    id: 'edge',
    name: 'Edge TTS',
    nativeSampleRate: 16000,    // Requested as raw-16khz-16bit-mono-pcm
    paramCount: 0,               // Cloud — unknown params
    runtime: 'cloud' as const,
    requiresInternet: true,
    requiresHfToken: false,
    voiceCloning: false,
    emotionTags: false,
    loraTrainable: false,
    defaultVoice: 'en-US-AriaNeural',
    voiceCount: 300,             // 300+ neural voices, 100+ languages
    description: 'Microsoft neural voices — 300+ distinct voices, <200ms, cloud',
  },

  /** Pocket-TTS — Kyutai's 100M CPU-native Candle TTS with voice cloning */
  POCKET: {
    id: 'pocket',
    name: 'Pocket TTS',
    hfRepo: 'kyutai/pocket-tts',
    hfVariant: 'b6369a24',
    hfWeightsFile: 'tts_b6369a24.safetensors',
    nativeSampleRate: 24000,     // Mimi codec native rate
    paramCount: 100_000_000,     // 100M
    modelSizeMB: 236,
    runtime: 'candle' as const,
    requiresInternet: false,     // After initial download
    requiresHfToken: true,       // Gated model — accept terms on HF first
    voiceCloning: true,          // From 5-15s WAV reference audio
    emotionTags: false,
    loraTrainable: false,        // Not LLM-based, uses FlowLM
    defaultVoice: 'alba',
    presetVoices: ['alba', 'fantine', 'cosette', 'eponine', 'azelma', 'marius', 'javert', 'jean'],
    voiceCount: 8,
    voiceDir: 'models/pocket-tts/voices',  // Place reference WAVs here
    description: 'Kyutai 100M Candle — fast CPU TTS, voice cloning from WAV reference',
  },

  /** Orpheus — 3B Llama-based TTS with emotion control */
  ORPHEUS: {
    id: 'orpheus',
    name: 'Orpheus TTS',
    hfRepo: 'canopylabs/orpheus-3b-0.1-ft',
    hfSnacRepo: 'hubertsiuzdak/snac_24khz',
    nativeSampleRate: 24000,     // SNAC codec native rate
    paramCount: 3_000_000_000,   // 3B
    modelSizeMB: 2048,           // Q4_K_M GGUF
    runtime: 'candle' as const,
    weightFormat: 'gguf' as const,
    requiresInternet: false,
    requiresHfToken: false,
    voiceCloning: false,
    emotionTags: true,           // <laugh> <sigh> <gasp> <cry>
    loraTrainable: true,         // Llama architecture — same LoRA pipeline
    architectureFamily: 'llama',
    defaultVoice: 'tara',
    presetVoices: ['tara', 'leah', 'jess', 'leo', 'dan', 'mia', 'zac', 'zoe'],
    voiceCount: 8,
    modelDir: 'models/orpheus',
    modelFiles: ['model-q4_k_m.gguf', 'tokenizer.json', 'snac_decoder.onnx'],
    description: 'Llama-3B GGUF — expressive with emotion tags, LoRA-trainable',
  },

  /** Kokoro — 82M ONNX TTS, fast offline */
  KOKORO: {
    id: 'kokoro',
    name: 'Kokoro TTS',
    hfRepo: 'onnx-community/Kokoro-82M-v1.0-ONNX',
    nativeSampleRate: 24000,
    paramCount: 82_000_000,      // 82M
    modelSizeMB: 330,
    runtime: 'onnx' as const,
    requiresInternet: false,
    requiresHfToken: false,
    voiceCloning: false,
    emotionTags: false,
    loraTrainable: false,        // ONNX — no LoRA injection
    defaultVoice: 'af',
    presetVoices: ['af', 'af_bella', 'af_nicole', 'af_sarah', 'af_sky', 'am_adam', 'am_michael', 'bf_emma', 'bf_isabella', 'bm_george', 'bm_lewis'],
    voiceCount: 11,
    modelDir: 'models/kokoro-v1.0',
    modelFiles: ['model.onnx', 'tokenizer.json', 'voices/'],
    description: 'ONNX 82M — fast offline fallback, ~97ms TTFB',
  },

  /** Piper — ONNX TTS (offline fallback) */
  PIPER: {
    id: 'piper',
    name: 'Piper TTS',
    hfRepo: 'rhasspy/piper-voices',
    nativeSampleRate: 22050,     // Most Piper models are 22050Hz
    paramCount: 0,               // Varies by voice model
    runtime: 'onnx' as const,
    requiresInternet: false,
    requiresHfToken: false,
    voiceCloning: false,
    emotionTags: false,
    loraTrainable: false,
    defaultVoice: 'default',
    voiceCount: 1,               // One model per voice
    modelDir: 'models/piper',
    modelFiles: ['model.onnx', 'model.onnx.json'],
    description: 'ONNX — offline fallback, production-grade',
  },

  /** Silence — testing adapter */
  SILENCE: {
    id: 'silence',
    name: 'Silence',
    nativeSampleRate: 16000,
    paramCount: 0,
    runtime: 'none' as const,
    requiresInternet: false,
    requiresHfToken: false,
    voiceCloning: false,
    emotionTags: false,
    loraTrainable: false,
    defaultVoice: 'default',
    voiceCount: 1,
    description: 'Testing adapter — generates silence',
  },
} as const;

/** Type for TTS model IDs */
export type TTSModelId = keyof typeof TTS_MODELS;

/**
 * STT_MODELS - SINGLE SOURCE OF TRUTH for Speech-to-Text models
 */
export const STT_MODELS = {
  WHISPER: {
    id: 'whisper',
    name: 'Whisper',
    hfRepo: 'ggerganov/whisper.cpp',
    hfModelFile: 'ggml-base.en.bin',
    modelDir: 'models/whisper',
    description: 'OpenAI Whisper via whisper.cpp — reliable, multilingual',
  },
  MOONSHINE: {
    id: 'moonshine',
    name: 'Moonshine',
    hfRepo: 'UsefulSensors/moonshine',
    modelDir: 'models/moonshine',
    description: 'ONNX — sub-100ms, great for live transcription',
  },
} as const;


/**
 * Command Names - SINGLE SOURCE OF TRUTH
 *
 * ⚠️ AUTO-GENERATED via generator/generate-command-constants.ts
 * ⚠️ Re-exported from shared/generated-command-constants.ts
 * ⚠️ NEVER hardcode command strings - use COMMANDS.* constants
 *
 * Usage:
 *   await Commands.execute(COMMANDS.DATA_LIST, params);
 *   if (command === COMMANDS.SESSION_CREATE) { ... }
 *
 * Exception: Only 'list' command may be hardcoded (bootstrap requirement)
 */
export { COMMANDS, CommandName } from '../../shared/generated-command-constants';


/**
 * ⚠️ DO NOT ADD FUNCTIONS THAT ACCESS process.env HERE ⚠️
 * This file is browser-safe and gets bundled into client code.
 * For runtime config that checks environment variables, use ServerConfig.ts
 */

// Re-export for backward compatibility (will be deprecated)
export { PATHS as DATABASE_PATHS };
