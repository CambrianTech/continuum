/**
 * Genome Command Constants
 *
 * All genome/*, lora/*, and runtime/* command names defined here.
 * Usage:
 *   import { GENOME_COMMANDS, LORA_COMMANDS, RUNTIME_COMMANDS } from './system/genome/shared/GenomeCommandConstants';
 *   await Commands.execute(GENOME_COMMANDS.LAYER.CREATE, params);
 */

/**
 * Genome layer and assembly commands
 */
export const GENOME_COMMANDS = {
  /** Genome layer management */
  LAYER: {
    /** Create new LoRA layer with metadata */
    CREATE: 'genome/layer/create',

    /** Find similar layers via cosine similarity */
    SEARCH: 'genome/layer/search',

    /** Trigger Academy refinement for existing layer */
    REFINE: 'genome/layer/refine',

    /** Delete genome layer */
    DELETE: 'genome/layer/delete',
  },

  /** Assemble complete genome from layers */
  ASSEMBLE: 'genome/assemble',

  /** Mount genome into runtime (load all required layers) */
  MOUNT: 'genome/mount',

  /** Unmount genome from runtime */
  UNMOUNT: 'genome/unmount',
} as const;

/**
 * LoRA adapter cache management commands
 */
export const LORA_COMMANDS = {
  /** Load LoRA adapter into model process */
  LOAD: 'lora/load',

  /** Evict LoRA adapter from cache */
  UNLOAD: 'lora/unload',

  /** Check cache state for process */
  STATUS: 'lora/status',
} as const;

/**
 * Model process runtime commands
 */
export const RUNTIME_COMMANDS = {
  /** Spawn new model process (worker/child process) */
  SPAWN_PROCESS: 'runtime/spawn-process',

  /** Kill running model process */
  KILL_PROCESS: 'runtime/kill-process',

  /** Execute inference with persona genome */
  EXECUTE_INFERENCE: 'runtime/execute-inference',

  /** Get status of all model processes */
  PROCESS_STATUS: 'runtime/process-status',
} as const;

/**
 * Type-safe genome command names
 */
export type GenomeLayerCommand = typeof GENOME_COMMANDS.LAYER[keyof typeof GENOME_COMMANDS.LAYER];
export type GenomeCommand = typeof GENOME_COMMANDS[Exclude<keyof typeof GENOME_COMMANDS, 'LAYER'>] | GenomeLayerCommand;
export type LoRACommand = typeof LORA_COMMANDS[keyof typeof LORA_COMMANDS];
export type RuntimeCommand = typeof RUNTIME_COMMANDS[keyof typeof RUNTIME_COMMANDS];

/**
 * All genome system commands
 */
export type GenomeSystemCommand = GenomeCommand | LoRACommand | RuntimeCommand;

/**
 * Similarity thresholds for genome layer matching
 * (from FINAL-ARCH-DECISIONS.md)
 */
export const SIMILARITY_THRESHOLDS = {
  /** Use layer as-is (no training needed) */
  USE_AS_IS: 0.90,

  /** Refine layer via Academy (8 hours) */
  REFINE: 0.75,

  /** Fork and adapt layer */
  FORK: 0.60,

  /** Train from scratch (<0.60) */
  TRAIN: 0.60,
} as const;

/**
 * Recommendation based on similarity score
 */
export type SimilarityRecommendation = 'use-as-is' | 'refine' | 'fork' | 'train';

/**
 * Get recommendation from similarity score
 */
export function getSimilarityRecommendation(similarity: number): SimilarityRecommendation {
  if (similarity >= SIMILARITY_THRESHOLDS.USE_AS_IS) return 'use-as-is';
  if (similarity >= SIMILARITY_THRESHOLDS.REFINE) return 'refine';
  if (similarity >= SIMILARITY_THRESHOLDS.FORK) return 'fork';
  return 'train';
}

/**
 * LoRA layer sources
 */
export const LAYER_SOURCES = {
  INHERITED: 'inherited',
  TRAINED: 'trained',
  REFINED: 'refined',
  DOWNLOADED: 'downloaded',
  SYSTEM: 'system',
} as const;

export type LayerSource = typeof LAYER_SOURCES[keyof typeof LAYER_SOURCES];

/**
 * Model process types
 */
export const PROCESS_TYPES = {
  WORKER: 'worker',
  CHILD_PROCESS: 'child_process',
} as const;

export type ProcessType = typeof PROCESS_TYPES[keyof typeof PROCESS_TYPES];

/**
 * Process status values
 */
export const PROCESS_STATUS = {
  READY: 'ready',
  BUSY: 'busy',
  INITIALIZING: 'initializing',
  CRASHED: 'crashed',
  RESTARTING: 'restarting',
} as const;

export type ProcessStatus = typeof PROCESS_STATUS[keyof typeof PROCESS_STATUS];

/**
 * Inference request priorities
 */
export const INFERENCE_PRIORITY = {
  IMMEDIATE: 'immediate',
  STANDARD: 'standard',
  DEFERRED: 'deferred',
} as const;

export type InferencePriority = typeof INFERENCE_PRIORITY[keyof typeof INFERENCE_PRIORITY];

/**
 * Adapter cache status
 */
export const ADAPTER_STATUS = {
  LOADED: 'loaded',
  EVICTED: 'evicted',
  LOADING: 'loading',
} as const;

export type AdapterStatus = typeof ADAPTER_STATUS[keyof typeof ADAPTER_STATUS];
