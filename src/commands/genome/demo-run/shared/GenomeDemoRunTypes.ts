/**
 * Genome Demo Run Command - Shared Types
 *
 * Launches a sentinel demo pipeline where Claude Code builds a real project
 * from a project spec, capturing all interactions for LoRA training.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Demo Run Command Parameters
 */
export interface GenomeDemoRunParams extends CommandParams {
  /** Project name (maps to src/projects/<name>/project.json) */
  project: string;

  /** Target persona ID to train with captured interactions */
  personaId: UUID;

  /** Persona display name (auto-resolved if not provided) */
  personaName?: string;

  /** Base model for LoRA training (default: LOCAL_MODELS.DEFAULT) */
  baseModel?: string;

  /** Max CodingAgent retries per milestone (default: 2) */
  maxRetries?: number;

  /** Max USD budget per milestone (default: 5.0) */
  maxBudget?: number;

  /** Max CodingAgent turns per milestone (default: 30) */
  maxTurns?: number;

  /** CodingAgent provider (default: 'claude-code') */
  provider?: string;

  /** LoRA training epochs (default: 3) */
  epochs?: number;

  /** LoRA rank (default: 32) */
  rank?: number;
}

/**
 * Factory function for creating GenomeDemoRunParams
 */
export const createGenomeDemoRunParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    project: string;
    personaId: UUID;
    personaName?: string;
    baseModel?: string;
    maxRetries?: number;
    maxBudget?: number;
    maxTurns?: number;
    provider?: string;
    epochs?: number;
    rank?: number;
  }
): GenomeDemoRunParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data,
});

/**
 * Genome Demo Run Command Result
 */
export interface GenomeDemoRunResult extends CommandResult {
  success: boolean;
  /** Sentinel handle for the pipeline */
  handle: string;
  /** Project name */
  projectName: string;
  /** Number of milestones in the project */
  milestoneCount: number;
  error?: string;
}

/**
 * Factory function for creating GenomeDemoRunResult with defaults
 */
export const createGenomeDemoRunResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    handle?: string;
    projectName?: string;
    milestoneCount?: number;
    error?: string;
  }
): GenomeDemoRunResult => createPayload(context, sessionId, {
  handle: data.handle ?? '',
  projectName: data.projectName ?? '',
  milestoneCount: data.milestoneCount ?? 0,
  ...data,
});

/**
 * Smart inheritance from params — auto-inherits context and sessionId
 */
export const createGenomeDemoRunResultFromParams = (
  params: GenomeDemoRunParams,
  differences: Omit<GenomeDemoRunResult, 'context' | 'sessionId' | 'userId'>
): GenomeDemoRunResult => transformPayload(params, differences);

/**
 * Genome Demo Run — Type-safe command executor
 */
export const GenomeDemoRun = {
  execute(params: CommandInput<GenomeDemoRunParams>): Promise<GenomeDemoRunResult> {
    return Commands.execute<GenomeDemoRunParams, GenomeDemoRunResult>(
      'genome/demo-run',
      params as Partial<GenomeDemoRunParams>
    );
  },
  commandName: 'genome/demo-run' as const,
} as const;
