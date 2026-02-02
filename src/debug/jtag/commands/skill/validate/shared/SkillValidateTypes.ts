/**
 * Skill Validate Command - Shared Types
 *
 * Validate a generated skill by running TypeScript compilation and tests in an ExecutionSandbox. Updates SkillEntity with validation results.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Skill Validate Command Parameters
 */
export interface SkillValidateParams extends CommandParams {
  // ID of the SkillEntity to validate
  skillId: string;
}

/**
 * Factory function for creating SkillValidateParams
 */
export const createSkillValidateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // ID of the SkillEntity to validate
    skillId: string;
  }
): SkillValidateParams => createPayload(context, sessionId, {

  ...data
});

/**
 * Skill Validate Command Result
 */
export interface SkillValidateResult extends CommandResult {
  success: boolean;
  // ID of the SkillEntity
  skillId: string;
  // Skill command name
  name: string;
  // Lifecycle status after validation
  status: string;
  // Whether TypeScript compilation succeeded
  compiled: boolean;
  // Number of tests executed
  testsRun: number;
  // Number of tests that passed
  testsPassed: number;
  // Array of error messages from compilation or tests
  errors: string[];
  // Human-readable result message
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating SkillValidateResult with defaults
 */
export const createSkillValidateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // ID of the SkillEntity
    skillId?: string;
    // Skill command name
    name?: string;
    // Lifecycle status after validation
    status?: string;
    // Whether TypeScript compilation succeeded
    compiled?: boolean;
    // Number of tests executed
    testsRun?: number;
    // Number of tests that passed
    testsPassed?: number;
    // Array of error messages from compilation or tests
    errors?: string[];
    // Human-readable result message
    message?: string;
    error?: JTAGError;
  }
): SkillValidateResult => createPayload(context, sessionId, {
  skillId: data.skillId ?? '',
  name: data.name ?? '',
  status: data.status ?? '',
  compiled: data.compiled ?? false,
  testsRun: data.testsRun ?? 0,
  testsPassed: data.testsPassed ?? 0,
  errors: data.errors ?? [],
  message: data.message ?? '',
  ...data
});

/**
 * Smart Skill Validate-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSkillValidateResultFromParams = (
  params: SkillValidateParams,
  differences: Omit<SkillValidateResult, 'context' | 'sessionId'>
): SkillValidateResult => transformPayload(params, differences);

/**
 * Skill Validate — Type-safe command executor
 *
 * Usage:
 *   import { SkillValidate } from '...shared/SkillValidateTypes';
 *   const result = await SkillValidate.execute({ ... });
 */
export const SkillValidate = {
  execute(params: CommandInput<SkillValidateParams>): Promise<SkillValidateResult> {
    return Commands.execute<SkillValidateParams, SkillValidateResult>('skill/validate', params as Partial<SkillValidateParams>);
  },
  commandName: 'skill/validate' as const,
} as const;
