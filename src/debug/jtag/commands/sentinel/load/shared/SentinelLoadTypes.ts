/**
 * Sentinel Load Command - Types
 *
 * Load and optionally run saved sentinel definitions from database.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { SentinelDefinition, SentinelEntity, SentinelExecutionResult } from '../../../../system/sentinel';

/**
 * Load params
 */
export interface SentinelLoadParams extends CommandParams {
  /** Sentinel entity ID or shortId */
  id: string;

  /** Run immediately after loading (default: false) */
  run?: boolean;

  /** Run asynchronously (default: true when run=true) */
  async?: boolean;

  /** Override working directory for this run */
  workingDir?: string;
}

/**
 * Load result
 */
export interface SentinelLoadResult extends CommandResult {
  /** Whether load succeeded */
  success: boolean;

  /** The loaded entity */
  entity?: SentinelEntity;

  /** If run=true, the execution handle */
  handle?: string;

  /** If run=true and async=false, the execution result */
  result?: SentinelExecutionResult;

  /** Error message if failed */
  error?: string;
}

/**
 * List params
 */
export interface SentinelListParams extends CommandParams {
  /** Filter by type */
  type?: 'build' | 'orchestrate' | 'screenshot' | 'task' | 'script';

  /** Filter by tags (any match) */
  tags?: string[];

  /** Only show templates */
  templatesOnly?: boolean;

  /** Limit results */
  limit?: number;
}

/**
 * List result
 */
export interface SentinelListResult extends CommandResult {
  success: boolean;
  sentinels: Array<{
    id: string;
    shortId: string;
    name: string;
    type: SentinelDefinition['type'];
    description?: string;
    tags?: string[];
    isTemplate?: boolean;
    executionCount: number;
    lastRun?: string;
    createdAt: string;
  }>;
  total: number;
}
