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
 * File System Paths - ALL paths in ONE place
 *
 * Priority for configurable paths:
 * 1. Explicit parameter (user provides full path)
 * 2. Environment variable (user overrides via process.env)
 * 3. These constants (our defaults)
 */
export const PATHS = {
  /** Base continuum directory */
  CONTINUUM: '.continuum',

  /** JTAG data storage */
  JTAG_DATA: '.continuum/jtag/data',
  JTAG_BACKUPS: '.continuum/jtag/backups',

  /** Main SQLite database */
  SQLITE_DB: '.continuum/jtag/data/database.sqlite',

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
  MEDIA_TEMP: '.continuum/media/temp',

  /** Legacy (for migration) */
  LEGACY_DB: '.continuum/database/continuum.db'
} as const;

/**
 * Environment Variables - standard names we check
 */
export const ENV_VARS = {
  DATASETS_DIR: 'DATASETS_DIR',
  REPO_PATH: 'REPO_PATH',
  SENTINEL_PATH: 'SENTINEL_PATH'
} as const;

/**
 * Collection Names - ALL entity collections
 */
export const COLLECTIONS = {
  USERS: 'users',
  USER_STATES: 'user_states',
  ROOMS: 'rooms',
  CHAT_MESSAGES: 'chat_messages',
  ARTIFACTS: 'artifacts',
  SESSIONS: 'sessions',
  TASKS: 'tasks',
  TRAINING_EXAMPLES: 'training_examples',
  TRAINING_SESSIONS: 'training_sessions',
  FINE_TUNING_JOBS: 'fine_tuning_jobs',
  FINE_TUNING_DATASETS: 'fine_tuning_datasets',
  FINE_TUNED_MODELS: 'fine_tuned_models',
  TRAINING_CHECKPOINTS: 'training_checkpoints',
  TRAINING_DATASETS: 'training_datasets',
  CODE_INDEX: 'code_index',

  // Cognition System Collections (Phase 1: Agent Architecture)
  PERSONA_SELF_STATE: 'persona_self_state',
  PERSONA_WORKING_MEMORY: 'persona_working_memory',
  PERSONA_EXPERIENCES: 'persona_experiences',
  PERSONA_PROCEDURES: 'persona_procedures',
  PERSONA_PLANS: 'persona_plans',
  PERSONA_LEARNINGS: 'persona_learnings',
  USER_PROFILES: 'user_profiles',

  // Cognition Observability Collections (Phase 1B: Monitoring)
  COGNITION_STATE_SNAPSHOTS: 'cognition_state_snapshots',
  COGNITION_PLAN_RECORDS: 'cognition_plan_records'
} as const;

/**
 * Database Configuration
 */
export const DB_CONFIG = {
  DEFAULT_LIMIT: 100,
  MAX_BATCH_SIZE: 500,
  CONNECTION_TIMEOUT: 10000,
  ENABLE_PERFORMANCE_LOGGING: false
} as const;

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
    SONNET_4_5: 'claude-sonnet-4-5-20250929',       // Current (Sep 2025)
    OPUS_3: 'claude-3-opus-20240229',
    HAIKU_3: 'claude-3-haiku-20240307'
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
    GROK_BETA: 'grok-beta',
    GROK_4: 'grok-4'
  },

  /** Ollama local models */
  OLLAMA: {
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
 * Helper: Get path with env var fallback
 * Usage: getPathWithEnvFallback('DATASETS_DIR', PATHS.DATASETS)
 */
export function getPathWithEnvFallback(envVarName: keyof typeof ENV_VARS, defaultPath: string): string {
  return process.env[ENV_VARS[envVarName]] || defaultPath;
}

/**
 * Helper: Resolve path priority (param > env > constant)
 */
export function resolvePath(explicitPath: string | undefined, envVarName: keyof typeof ENV_VARS, defaultPath: string): string {
  return explicitPath ?? getPathWithEnvFallback(envVarName, defaultPath);
}

// Re-export for backward compatibility (will be deprecated)
export { PATHS as DATABASE_PATHS };
export type CollectionName = typeof COLLECTIONS[keyof typeof COLLECTIONS];
