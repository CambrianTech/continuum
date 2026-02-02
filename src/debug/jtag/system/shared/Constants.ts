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
 * Both browser and server need to know collection names
 */
export const COLLECTIONS = {
  USERS: 'users',
  USER_STATES: 'user_states',
  ROOMS: 'rooms',
  CHAT_MESSAGES: 'chat_messages',
  ARTIFACTS: 'artifacts',
  SESSIONS: 'sessions',
  TASKS: 'tasks',
  PINNED_ITEMS: 'pinned_items',
  COORDINATION_DECISIONS: 'coordination_decisions',
  TRAINING_EXAMPLES: 'training_examples',
  TRAINING_SESSIONS: 'training_sessions',
  FINE_TUNING_JOBS: 'fine_tuning_jobs',
  FINE_TUNING_DATASETS: 'fine_tuning_datasets',
  FINE_TUNED_MODELS: 'fine_tuned_models',
  TRAINING_CHECKPOINTS: 'training_checkpoints',
  TRAINING_DATASETS: 'training_datasets',
  CODE_INDEX: 'code_index',

  // Room Wall System
  WALL_DOCUMENTS: 'wall_documents',

  // Memory System (Phase 2)
  MEMORIES: 'memories',

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
  COGNITION_PLAN_RECORDS: 'cognition_plan_records',

  // Detailed Activity Logs (Phase 2: Complete Observability)
  TOOL_EXECUTION_LOGS: 'tool_execution_logs',
  ADAPTER_DECISION_LOGS: 'adapter_decision_logs',
  RESPONSE_GENERATION_LOGS: 'response_generation_logs',

  // Granular Cognitive Logs (Phase 3: Deep Observability)
  COGNITION_PLAN_STEP_EXECUTIONS: 'cognition_plan_step_executions',
  COGNITION_SELF_STATE_UPDATES: 'cognition_self_state_updates',
  COGNITION_MEMORY_OPERATIONS: 'cognition_memory_operations',
  ADAPTER_REASONING_LOGS: 'adapter_reasoning_logs',
  COGNITION_PLAN_REPLANS: 'cognition_plan_replans',

  // Universal Democratic Voting System
  VOTING_PROPOSALS: 'voting_proposals',              // All votable proposals (universal)
  PERMISSION_ELEVATION_PROPOSALS: 'permission_elevation_proposals',
  PERMISSION_DEMOTION_PROPOSALS: 'permission_demotion_proposals',

  // Legacy voting collections (for backward compatibility - migrate to VOTING_PROPOSALS)
  FILE_VOTE_PROPOSALS: 'file_vote_proposals',
  DECISION_PROPOSALS: 'decision_proposals',

  // AI Governance and Permission System
  MUTE_STATUS: 'mute_status',                        // Active mutes
  PERMISSION_HISTORY: 'permission_history',          // Track AI progression/demotion
  USER_METRICS: 'user_metrics',                      // Performance tracking for governance
  ROOM_PERMISSIONS: 'room_permissions',              // Per-room access control
  EXPERTISE_TOKENS: 'expertise_tokens',              // Domain expertise recognition (AI-suggested)
  POST_VOTE_DEBRIEFS: 'post_vote_debriefs',         // Learning from votes (AI-suggested)
  MENTORSHIP_RELATIONSHIPS: 'mentorship_relationships', // AI mentorship system (AI-suggested)

  // Collaborative Editing System (Lease Daemon)
  FILE_LEASES: 'file_leases',
  LEASE_QUEUES: 'lease_queues',
  APPROVAL_REQUESTS: 'approval_requests',
  RELEASE_REQUESTS: 'release_requests',
  KICK_VOTES: 'kick_votes',
  KICK_APPEALS: 'kick_appeals',

  // Collaborative Canvas System
  CANVAS_STROKES: 'canvas_strokes',

  // Activity System - collaborative content instances (canvas, browser, games, etc.)
  ACTIVITIES: 'activities',

  // Universal Handle System — persistent async operation references
  HANDLES: 'handles',

  // Coding Agent System (Phase 4: Multi-Agent Coordination)
  CODING_PLANS: 'coding_plans',

  // Self-Modifying Skills (Phase 4B: AI-Created Commands)
  SKILLS: 'skills',

  // Coding Challenges & Learning (Phase 4D: Progressive Training)
  CODING_CHALLENGES: 'coding_challenges',
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

  /** Ollama local models (legacy - use LOCAL_MODELS for new code) */
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
 * LOCAL_MODELS - SINGLE SOURCE OF TRUTH for local inference
 *
 * ⚠️ CRITICAL: This is the canonical model configuration for Candle (native Rust) inference
 * ⚠️ All model mappings, preloads, and defaults come from here
 * ⚠️ CandleAdapter reads from here - DO NOT duplicate mappings elsewhere
 *
 * OLLAMA IS REMOVED: Candle is the ONLY local inference path.
 * The model name mappings below exist for backward compatibility with
 * configs that reference Ollama-style names like 'llama3.2:3b'.
 *
 * Note: Using Qwen models as defaults because Meta's Llama requires HuggingFace access approval
 * To use real Llama: accept license at https://huggingface.co/meta-llama
 */
export const LOCAL_MODELS = {
  /** Default models for inference worker to preload at startup */
  PRELOAD: [
    'Qwen/Qwen2-1.5B-Instruct',  // Main local model (used by llama3.2:3b personas)
    'Qwen/Qwen2-0.5B-Instruct',  // Fast model for gating/classification
  ],

  /** Default model for local inference */
  DEFAULT: 'Qwen/Qwen2-1.5B-Instruct',

  /** Fast model for gating/classification tasks */
  GATING: 'Qwen/Qwen2-0.5B-Instruct',

  /** Map legacy model names → HuggingFace model IDs (Ollama naming style kept for backward compat) */
  OLLAMA_TO_HUGGINGFACE: {
    // Llama 3.2 family → Qwen fallback (Llama requires HF approval)
    'llama3.2:3b': 'Qwen/Qwen2-1.5B-Instruct',
    'llama3.2:1b': 'Qwen/Qwen2-0.5B-Instruct',
    'llama3.2-3b': 'Qwen/Qwen2-1.5B-Instruct',
    'llama3.2-1b': 'Qwen/Qwen2-0.5B-Instruct',

    // Llama 3.1 family (requires HF approval)
    'llama3.1:8b': 'meta-llama/Llama-3.1-8B-Instruct',
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
  } as const,

  /**
   * Map a model name to HuggingFace ID
   * Returns original if not found (might already be a HuggingFace ID)
   */
  mapToHuggingFace(modelName: string): string {
    const normalized = modelName.toLowerCase().trim();
    const mapping = LOCAL_MODELS.OLLAMA_TO_HUGGINGFACE as Record<string, string>;

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
export type CollectionName = typeof COLLECTIONS[keyof typeof COLLECTIONS];
