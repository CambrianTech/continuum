/**
 * Run sentinels for builds, orchestration, screenshots, tasks, or declarative pipelines.
 * Uses handles for long-running operations and emits events for progress.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { ModelCapacity, ModelProvider } from '../../../../system/sentinel/ModelProvider';

import type { PipelineSentinelDefinition } from '../../../../system/sentinel/SentinelDefinition';

/**
 * Sentinel types available
 */
export type SentinelType = 'build' | 'orchestrate' | 'screenshot' | 'task' | 'pipeline';

/**
 * Run sentinels for builds, orchestration, screenshots, tasks, or declarative pipelines.
 */
export interface SentinelRunParams extends CommandParams {
  /** Type of sentinel to run */
  type: SentinelType;

  /** Working directory (defaults to JTAG root) */
  workingDir?: string;

  /** Timeout in ms (defaults to 120000) */
  timeout?: number;

  /** Whether to run async with handle (default true for long operations) */
  async?: boolean;

  /** If this sentinel was saved, its entity ID for lifecycle tracking */
  entityId?: string;

  /** Owning persona — for escalation routing when sentinel finishes */
  parentPersonaId?: string;

  /** Sentinel name — for human-readable escalation messages */
  sentinelName?: string;
}

/**
 * BuildSentinel params
 */
export interface BuildSentinelParams extends SentinelRunParams {
  type: 'build';

  /** Build command to run */
  command: string;

  /** Max fix attempts */
  maxAttempts?: number;

  /** Whether to attempt auto-fixes */
  canAutoFix?: boolean;

  /** Enable LLM-assisted error fixing when pattern matching fails */
  useLLM?: boolean;

  /** LLM capacity for error analysis (SMALL recommended for speed) */
  capacity?: ModelCapacity;

  /** LLM provider (LOCAL, CANDLE, ANTHROPIC, etc.) */
  provider?: ModelProvider;
}

/**
 * OrchestratorSentinel params
 */
export interface OrchestrateSentinelParams extends SentinelRunParams {
  type: 'orchestrate';

  /** The goal to accomplish */
  goal: string;

  /** Max iterations */
  maxIterations?: number;

  /** Model capacity level */
  capacity?: ModelCapacity;

  /** Model provider */
  provider?: ModelProvider;

  /** Specific model name */
  modelName?: string;

  /** Where to save screenshots */
  screenshotDir?: string;
}

/**
 * VisualSentinel params
 */
export interface ScreenshotSentinelParams extends SentinelRunParams {
  type: 'screenshot';

  /** File path or URL to screenshot */
  target: string;

  /** Output filename */
  filename?: string;

  /** Output directory */
  outputDir?: string;

  /** Viewport size */
  viewport?: { width: number; height: number };
}

/**
 * TaskSentinel params
 */
export interface TaskSentinelParams extends SentinelRunParams {
  type: 'task';

  /** Tasks to execute (JSON array) */
  tasks: Array<{
    name: string;
    action: 'write' | 'read' | 'run' | 'build';
    file?: string;
    content?: string;
    command?: string;
  }>;

  /** Max recursion depth */
  maxDepth?: number;

  /** Max total tasks */
  maxTotalTasks?: number;
}

/**
 * PipelineSentinel params (declarative step-based execution)
 */
export interface PipelineSentinelParams extends SentinelRunParams {
  type: 'pipeline';

  /** Pipeline definition (JSON) */
  definition: PipelineSentinelDefinition;
}

/**
 * Union of all sentinel param types
 */
export type AnySentinelParams =
  | BuildSentinelParams
  | OrchestrateSentinelParams
  | ScreenshotSentinelParams
  | TaskSentinelParams
  | PipelineSentinelParams;

/**
 * Sentinel result data (internal, without JTAGPayload fields)
 */
export interface SentinelResultData {
  /** Whether the sentinel succeeded */
  success: boolean;

  /** Handle ID for async operations */
  handle?: string;

  /** Whether operation completed (false if async) */
  completed: boolean;

  /** Combined output from sync execution (only when async=false) */
  output?: string;

  /** Sentinel-specific result data */
  data?: {
    success: boolean;
    summary?: string;
    filesCreated?: string[];
    filesModified?: string[];
    errors?: string[];
    screenshot?: string;
    attempts?: number;
    iterations?: number;
    // Pipeline-specific fields
    stepResults?: unknown[];
    stepsCompleted?: number;
    stepsTotal?: number;
    durationMs?: number;
    error?: string;
  };
}

/**
 * Sentinel run result (full, with JTAGPayload)
 */
export interface SentinelRunResult extends CommandResult, SentinelResultData {}

/**
 * Events emitted during sentinel execution
 */
export interface SentinelEvents {
  /** Progress update */
  'sentinel:progress': {
    handle: string;
    type: SentinelType;
    step: string;
    progress: number;  // 0-100
    message: string;
  };

  /** Sentinel completed */
  'sentinel:complete': {
    handle: string;
    type: SentinelType;
    success: boolean;
    data: SentinelRunResult['data'];
  };

  /** Sentinel error */
  'sentinel:error': {
    handle: string;
    type: SentinelType;
    error: string;
  };

  /** File created by sentinel */
  'sentinel:file:created': {
    handle: string;
    type: SentinelType;
    path: string;
    size: number;
  };

  /** Screenshot taken */
  'sentinel:screenshot': {
    handle: string;
    type: SentinelType;
    path: string;
  };
}

/**
 * Handle status query params
 */
export interface SentinelStatusParams extends CommandParams {
  handle: string;
}

/**
 * Handle status result
 */
export interface SentinelStatusResult extends CommandResult {
  handle: string;
  type: SentinelType;
  status: 'running' | 'completed' | 'failed' | 'not_found';
  progress?: number;
  data?: SentinelRunResult['data'];
  error?: string;
}
