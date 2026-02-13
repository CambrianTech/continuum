/**
 * Sentinel List Command - Types
 *
 * List saved sentinel definitions from database.
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { SentinelDefinition } from '../../../../system/sentinel';

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

  /** Limit results (default: 20) */
  limit?: number;

  /** Search by name */
  search?: string;
}

/**
 * Summary of a sentinel for listing
 */
export interface SentinelSummary {
  id: string;
  shortId: string;
  name: string;
  type: SentinelDefinition['type'];
  description?: string;
  tags?: string[];
  isTemplate?: boolean;
  executionCount: number;
  lastRun?: string;
  lastSuccess?: boolean;
  createdAt: string;
  createdBy?: string;
}

/**
 * List result
 */
export interface SentinelListResult extends CommandResult {
  success: boolean;
  sentinels: SentinelSummary[];
  total: number;
}
