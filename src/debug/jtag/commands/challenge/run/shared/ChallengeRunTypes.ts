/**
 * Challenge Run Command - Shared Types
 *
 * Run a coding challenge against the AI coding pipeline. Sets up a fresh workspace, executes the challenge via code/task, evaluates with AI judge, and records the attempt.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Challenge Run Command Parameters
 */
export interface ChallengeRunParams extends CommandParams {
  // Specific challenge ID to run. If not provided, runs the next unbeaten challenge
  challengeId?: string;
  // Run challenge by sequence number (1-5)
  challengeNumber?: number;
  // Which AI persona runs the challenge. Defaults to the calling user
  personaId?: string;
  // Skip AI judge evaluation (faster, just checks execution success)
  skipJudge?: boolean;
}

/**
 * Factory function for creating ChallengeRunParams
 */
export const createChallengeRunParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Specific challenge ID to run. If not provided, runs the next unbeaten challenge
    challengeId?: string;
    // Run challenge by sequence number (1-5)
    challengeNumber?: number;
    // Which AI persona runs the challenge. Defaults to the calling user
    personaId?: string;
    // Skip AI judge evaluation (faster, just checks execution success)
    skipJudge?: boolean;
  }
): ChallengeRunParams => createPayload(context, sessionId, {
  challengeId: data.challengeId ?? '',
  challengeNumber: data.challengeNumber ?? 0,
  personaId: data.personaId ?? '',
  skipJudge: data.skipJudge ?? false,
  ...data
});

/**
 * Challenge Run Command Result
 */
export interface ChallengeRunResult extends CommandResult {
  success: boolean;
  // Name of the challenge that was run
  challengeName: string;
  // Challenge difficulty level
  difficulty: string;
  // Attempt outcome: passed, failed, partial, timeout, error
  status: string;
  // Judge score from 0-100
  score: number;
  // Judge feedback on the attempt
  feedback: string;
  // Total execution time in milliseconds
  durationMs: number;
  // Number of tool calls consumed
  toolCallsUsed: number;
  // Files modified during the attempt
  filesModified: string[];
  // Files created during the attempt
  filesCreated: string[];
  // Errors encountered during execution
  errors: string[];
  error?: JTAGError;
}

/**
 * Factory function for creating ChallengeRunResult with defaults
 */
export const createChallengeRunResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Name of the challenge that was run
    challengeName?: string;
    // Challenge difficulty level
    difficulty?: string;
    // Attempt outcome: passed, failed, partial, timeout, error
    status?: string;
    // Judge score from 0-100
    score?: number;
    // Judge feedback on the attempt
    feedback?: string;
    // Total execution time in milliseconds
    durationMs?: number;
    // Number of tool calls consumed
    toolCallsUsed?: number;
    // Files modified during the attempt
    filesModified?: string[];
    // Files created during the attempt
    filesCreated?: string[];
    // Errors encountered during execution
    errors?: string[];
    error?: JTAGError;
  }
): ChallengeRunResult => createPayload(context, sessionId, {
  challengeName: data.challengeName ?? '',
  difficulty: data.difficulty ?? '',
  status: data.status ?? '',
  score: data.score ?? 0,
  feedback: data.feedback ?? '',
  durationMs: data.durationMs ?? 0,
  toolCallsUsed: data.toolCallsUsed ?? 0,
  filesModified: data.filesModified ?? [],
  filesCreated: data.filesCreated ?? [],
  errors: data.errors ?? [],
  ...data
});

/**
 * Smart Challenge Run-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createChallengeRunResultFromParams = (
  params: ChallengeRunParams,
  differences: Omit<ChallengeRunResult, 'context' | 'sessionId'>
): ChallengeRunResult => transformPayload(params, differences);

/**
 * Challenge Run — Type-safe command executor
 *
 * Usage:
 *   import { ChallengeRun } from '...shared/ChallengeRunTypes';
 *   const result = await ChallengeRun.execute({ ... });
 */
export const ChallengeRun = {
  execute(params: CommandInput<ChallengeRunParams>): Promise<ChallengeRunResult> {
    return Commands.execute<ChallengeRunParams, ChallengeRunResult>('challenge/run', params as Partial<ChallengeRunParams>);
  },
  commandName: 'challenge/run' as const,
} as const;
