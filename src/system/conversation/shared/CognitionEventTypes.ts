/**
 * Cognition Event Types - Pipeline metrics for visualization
 *
 * These events enable Winamp-style frequency histogram visualization
 * of AI cognitive pipeline stages.
 */

import type { UUID } from '../../core/types/JTAGTypes';

/**
 * Cognition Event Names
 * Emitted during AI cognitive pipeline execution
 */
export const COGNITION_EVENTS = {
  /** Single pipeline stage completed */
  STAGE_COMPLETE: 'cognition:stage-complete',

  /** Complete pipeline summary for a message */
  PIPELINE_SUMMARY: 'cognition:pipeline-summary'
} as const;

export type CognitionEventType = typeof COGNITION_EVENTS[keyof typeof COGNITION_EVENTS];

/**
 * Pipeline stage identifiers
 */
export type PipelineStage =
  | 'rag-build'          // Context gathering
  | 'should-respond'     // Decision making
  | 'generate'           // LLM inference
  | 'coordination'       // ThoughtStream decision
  | 'post-response';     // Message delivery

/**
 * Stage performance status
 */
export type StageStatus = 'fast' | 'normal' | 'slow' | 'bottleneck';

/**
 * Metrics for a single pipeline stage
 */
export interface StageMetrics {
  /** Stage identifier */
  stage: PipelineStage;

  /** Time taken for this stage (ms) */
  durationMs: number;

  /** Resource used (stage-specific: tokens, confidence, etc) */
  resourceUsed: number;

  /** Maximum resource capacity */
  maxResource: number;

  /** Resource usage as percentage (0-100) */
  percentCapacity: number;

  /** Speed relative to baseline (0-100, 100=instant) */
  percentSpeed: number;

  /** Performance status */
  status: StageStatus;

  /** Optional stage-specific metadata */
  metadata?: Record<string, unknown>;
}

/**
 * Event: Single pipeline stage completed
 */
export interface StageCompleteEvent {
  /** Message being processed */
  messageId: UUID;

  /** Persona performing this stage */
  personaId: UUID;

  /** Conversation context */
  contextId: UUID;

  /** Stage that completed */
  stage: PipelineStage;

  /** Performance metrics */
  metrics: StageMetrics;

  /** Timestamp */
  timestamp: number;
}

/**
 * Event: Complete pipeline summary
 */
export interface PipelineSummaryEvent {
  /** Message being processed */
  messageId: UUID;

  /** Persona that completed pipeline */
  personaId: UUID;

  /** Conversation context */
  contextId: UUID;

  /** All stage metrics */
  stages: {
    ragBuild?: StageMetrics;
    shouldRespond?: StageMetrics;
    generate?: StageMetrics;
    coordination?: StageMetrics;
    postResponse?: StageMetrics;
  };

  /** Aggregate totals */
  totals: {
    /** Total latency across all stages */
    latencyMs: number;

    /** Which stage was slowest */
    bottleneck: PipelineStage;

    /** Aggregate compute score (0-100) */
    computeScore: number;
  };

  /** Timestamp */
  timestamp: number;
}

/**
 * Baseline speeds for performance grading
 * Durations under baseline = "fast", over 2x baseline = "slow"
 */
export const BASELINE_SPEEDS: Record<PipelineStage, number> = {
  'rag-build': 500,        // < 500ms = fast
  'should-respond': 200,   // < 200ms = fast
  'generate': 1000,        // < 1s = fast
  'coordination': 300,     // < 300ms = fast
  'post-response': 100     // < 100ms = fast
};

/**
 * Calculate speed score (0-100) based on duration
 *
 * @param durationMs - Actual duration
 * @param stage - Pipeline stage
 * @returns Score where 100=instant, 0=very slow
 */
export function calculateSpeedScore(durationMs: number, stage: PipelineStage): number {
  const baseline = BASELINE_SPEEDS[stage];
  // Score: 100 at 0ms, 0 at 2x baseline
  const score = Math.max(0, Math.min(100, (1 - durationMs / (baseline * 2)) * 100));
  return Math.round(score);
}

/**
 * Determine stage status based on duration
 *
 * @param durationMs - Actual duration
 * @param stage - Pipeline stage
 * @returns Performance status
 */
export function getStageStatus(durationMs: number, stage: PipelineStage): StageStatus {
  const baseline = BASELINE_SPEEDS[stage];

  if (durationMs < baseline) return 'fast';
  if (durationMs < baseline * 1.5) return 'normal';
  if (durationMs < baseline * 2) return 'slow';
  return 'bottleneck';
}

/**
 * Get color gradient for speed visualization
 *
 * @param speedPercent - Speed score (0-100)
 * @returns CSS gradient string
 */
export function getSpeedColor(speedPercent: number): string {
  // Fast (green) → Normal (yellow) → Slow (red)
  if (speedPercent >= 80) return 'linear-gradient(90deg, #0f0, #0ff)';  // Green-Cyan
  if (speedPercent >= 50) return 'linear-gradient(90deg, #ff0, #fa0)';  // Yellow-Orange
  return 'linear-gradient(90deg, #fa0, #f00)';                          // Orange-Red
}
