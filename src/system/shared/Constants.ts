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
 * File System Paths - Browser-safe constants only
 *
 * ⚠️ DO NOT add database paths or other server-only paths here!
 * ⚠️ This file is bundled into browser code
 * ⚠️ Use DatabaseConfig.ts for server-only database paths
 */
export const PATHS = {
  /** Base continuum directory */
  CONTINUUM: '.continuum',

  /** Training datasets directory - users can override with DATASETS_DIR env var for external drives */
  DATASETS: '.continuum/datasets',
  DATASETS_PARSED: '.continuum/datasets/parsed',
  DATASETS_PREPARED: '.continuum/datasets/prepared',

  /** Logs */
  LOGS: '.continuum/logs',

  /** Screenshots */
  SCREENSHOTS: '.continuum/screenshots',

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
    HAIKU_3_5: 'claude-3-5-haiku-20241022'          // Claude 3.5 Haiku (Oct 2024)
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

  /** Candle local models (use LOCAL_MODELS for new code) */
  CANDLE: {
    LLAMA_3_2_3B: 'llama3.2:3b',
    LLAMA_3_1_8B: 'llama3.1:8b'
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
 * ⚠️ CRITICAL: This is the canonical model configuration for Candle (native Rust) inference
 * ⚠️ All model mappings, preloads, and defaults come from here
 * ⚠️ CandleAdapter reads from here - DO NOT duplicate mappings elsewhere
 *
 * Candle is the ONLY local inference path.
 * The model name mappings below exist for backward compatibility with
 * configs that reference legacy short names like 'llama3.2:3b'.
 *
 * Note: Using unsloth/ mirrors for Llama models (no HuggingFace access approval needed)
 * For meta-llama/ originals: accept license at https://huggingface.co/meta-llama
 */
export const LOCAL_MODELS = {
  /** Default models for inference worker to preload at startup */
  PRELOAD: [
    'unsloth/Llama-3.2-3B-Instruct',  // Default model for inference + training
    'Qwen/Qwen2-0.5B-Instruct',       // Fast model for gating/classification
  ],

  /** Default model for local inference AND training.
   *  CRITICAL: This MUST match CandleAdapter's default_model in candle_adapter.rs.
   *  LoRA adapters trained on one model CANNOT work on a different architecture.
   *  Using unsloth/ mirror because meta-llama/ requires HuggingFace access approval. */
  DEFAULT: 'unsloth/Llama-3.2-3B-Instruct',

  /** Fast model for gating/classification tasks */
  GATING: 'Qwen/Qwen2-0.5B-Instruct',

  /** Map legacy model names → HuggingFace model IDs (legacy naming style kept for backward compat) */
  LEGACY_TO_HUGGINGFACE: {
    // Llama 3.2 family — uses unsloth mirror (no HF approval needed)
    'llama3.2:3b': 'unsloth/Llama-3.2-3B-Instruct',
    'llama3.2:1b': 'Qwen/Qwen2-0.5B-Instruct',  // Keep 1B small for gating
    'llama3.2-3b': 'unsloth/Llama-3.2-3B-Instruct',
    'llama3.2-1b': 'Qwen/Qwen2-0.5B-Instruct',

    // Llama 3.1 family
    'llama3.1:8b': 'unsloth/Llama-3.1-8B-Instruct',
    'llama3.1:70b': 'meta-llama/Llama-3.1-70B-Instruct',

    // Phi family (Microsoft, no approval needed)
    'phi3:mini': 'microsoft/Phi-3-mini-4k-instruct',
    'phi3:small': 'microsoft/Phi-3-small-8k-instruct',
    'phi3:medium': 'microsoft/Phi-3-medium-4k-instruct',
    'phi:2': 'microsoft/phi-2',
    'phi3': 'microsoft/Phi-3-mini-4k-instruct',

    // Mistral family (no approval needed)
    'mistral:7b': 'mistralai/Mistral-7B-Instruct-v0.2',
    'mistral:7b-v0.3': 'mistralai/Mistral-7B-Instruct-v0.3',
    'mixtral:8x7b': 'mistralai/Mixtral-8x7B-Instruct-v0.1',
    'mistral': 'mistralai/Mistral-7B-Instruct-v0.2',

    // Qwen family (no approval needed - recommended!)
    'qwen2:0.5b': 'Qwen/Qwen2-0.5B-Instruct',
    'qwen2:1.5b': 'Qwen/Qwen2-1.5B-Instruct',
    'qwen2:7b': 'Qwen/Qwen2-7B-Instruct',
    'qwen2.5:7b': 'Qwen/Qwen2.5-7B-Instruct',
    'qwen2.5:3b': 'Qwen/Qwen2.5-3B-Instruct',
    'qwen2': 'Qwen/Qwen2-0.5B-Instruct',

    // Gemma family (Google, no approval needed)
    'gemma:2b': 'google/gemma-2b-it',
    'gemma:7b': 'google/gemma-7b-it',
    'gemma2:2b': 'google/gemma-2-2b-it',
    'gemma2:9b': 'google/gemma-2-9b-it',

    // StarCoder family
    'starcoder2:3b': 'bigcode/starcoder2-3b',
    'starcoder2:7b': 'bigcode/starcoder2-7b',

    // TinyLlama (good for testing)
    'tinyllama': 'TinyLlama/TinyLlama-1.1B-Chat-v1.0',
    'tinyllama:1.1b': 'TinyLlama/TinyLlama-1.1B-Chat-v1.0',

    // SmolLM2 family (HuggingFace, good for fast testing)
    'smollm2:135m': 'HuggingFaceTB/SmolLM2-135M-Instruct',
    'smollm2:360m': 'HuggingFaceTB/SmolLM2-360M-Instruct',
    'smollm2:1.7b': 'HuggingFaceTB/SmolLM2-1.7B-Instruct',

    // Bare family aliases (resolve to default variant)
    'llama3.2': 'unsloth/Llama-3.2-3B-Instruct',
    'llama3.1': 'unsloth/Llama-3.1-8B-Instruct',
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

    // Try without version suffix (e.g., 'llama3.2:3b-instruct' -> 'llama3.2:3b')
    const withoutSuffix = normalized.replace(/-instruct.*$|-chat.*$|-q\d+.*$/i, '');
    if (mapping[withoutSuffix]) {
      return mapping[withoutSuffix];
    }

    // Not found - assume it's already a HuggingFace ID
    return modelName;
  }
} as const;

/**

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
