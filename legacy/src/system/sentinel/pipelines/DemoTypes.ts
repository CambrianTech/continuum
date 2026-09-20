/**
 * DemoTypes — Types for the Sentinel Demo Pipeline system
 *
 * The demo pipeline has Claude Code build real software inside sentinel pipelines,
 * capturing every interaction for LoRA training. This is fundamentally different from
 * the Academy teacher/student architecture:
 *
 * - Academy: local model (baseModel) attempts coding → teacher grades → LoRA trains local model
 * - Demo: Claude Code (cloud) builds production code → interactions captured → LoRA trains local model
 *
 * The demo proves that a professional-grade AI building real software generates
 * high-quality training data that measurably improves local persona capabilities.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { MilestoneSpec, ProjectSpec, AcademyConfig } from '../../genome/shared/AcademyTypes';

// ============================================================================
// Demo Pipeline Configuration
// ============================================================================

/**
 * Configuration for a demo pipeline run.
 *
 * The pipeline builder reads project.json at build time and generates
 * a fixed step sequence — one CodingAgent + Shell test loop per milestone.
 */
export interface DemoPipelineConfig {
  /** Path to project directory containing project.json */
  projectDir: string;

  /** Parsed project spec (read from project.json) */
  project: ProjectSpec;

  /** Target persona to train with captured interactions */
  personaId: UUID;

  /** Persona display name (for logging and adapter naming) */
  personaName: string;

  /** Base model for LoRA training (e.g., 'unsloth/Llama-3.2-3B-Instruct') */
  baseModel: string;

  /** Max CodingAgent retries per milestone when tests fail (default: 2) */
  maxRetries: number;

  /** Max USD budget per milestone for CodingAgent (default: 5.0) */
  maxBudgetPerMilestone: number;

  /** Max turns per CodingAgent invocation (default: 30) */
  maxTurnsPerMilestone: number;

  /** CodingAgent provider (default: 'claude-code') */
  provider: string;

  /** LoRA training config */
  training: DemoTrainingConfig;
}

/**
 * LoRA training configuration for captured interactions.
 */
export interface DemoTrainingConfig {
  /** Training epochs (default: 3) */
  epochs: number;

  /** LoRA rank (default: 32) */
  rank: number;

  /** Learning rate (default: 0.0001) */
  learningRate: number;

  /** Batch size (default: 4) */
  batchSize: number;
}

/**
 * Default demo training configuration.
 */
export const DEFAULT_DEMO_TRAINING_CONFIG: DemoTrainingConfig = {
  epochs: 3,
  rank: 32,
  learningRate: 0.0001,
  batchSize: 4,
};

/**
 * Default demo pipeline configuration values.
 */
export const DEMO_DEFAULTS = {
  maxRetries: 2,
  maxBudgetPerMilestone: 5.0,
  maxTurnsPerMilestone: 30,
  provider: 'claude-code',
} as const;

// ============================================================================
// Demo Pipeline Result
// ============================================================================

/**
 * Result from a completed demo pipeline run.
 * Emitted as the final step's payload.
 */
export interface DemoResult {
  /** Project name */
  projectName: string;

  /** Total milestones attempted */
  milestonesAttempted: number;

  /** Milestones that passed tests */
  milestonesPassed: number;

  /** Per-milestone results */
  milestoneResults: DemoMilestoneResult[];

  /** Total estimated cost in USD */
  totalCostUsd: number;

  /** Total interactions captured for training */
  totalInteractionsCaptured: number;

  /** Training result (if training ran) */
  trainingResult?: {
    layerId: string;
    finalLoss: number;
    trainingTimeMs: number;
    examplesProcessed: number;
  };

  /** Phenotype validation result (if validation ran) */
  phenotypeResult?: {
    beforeScore: number;
    afterScore: number;
    improvement: number;
  };
}

/**
 * Result for a single milestone within the demo.
 */
export interface DemoMilestoneResult {
  /** Milestone index */
  index: number;

  /** Milestone name */
  name: string;

  /** Whether tests passed */
  passed: boolean;

  /** Number of CodingAgent attempts (1 = first try, 2+ = retried) */
  attempts: number;

  /** Interactions captured from this milestone */
  interactionsCaptured: number;
}

// ============================================================================
// Demo Event Names
// ============================================================================

/**
 * Generate a scoped demo event name.
 * Pattern: `demo:{runId}:{action}`
 */
export function demoEvent(runId: string, action: DemoEventAction): string {
  return `demo:${runId}:${action}`;
}

export type DemoEventAction =
  | 'setup:complete'
  | 'milestone:start'
  | 'milestone:passed'
  | 'milestone:failed'
  | 'milestone:complete'
  | 'training:start'
  | 'training:complete'
  | 'validation:complete'
  | 'demo:complete';
