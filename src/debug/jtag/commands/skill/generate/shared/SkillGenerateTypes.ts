/**
 * Skill Generate Command - Shared Types
 *
 * Generate code files for a proposed skill using the CommandGenerator. Retrieves the SkillEntity and produces source files.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Skill Generate Command Parameters
 */
export interface SkillGenerateParams extends CommandParams {
  // ID of the SkillEntity to generate code for
  skillId: string;
  // Override output directory (default: persona workspace or commands/)
  outputDir?: string;
}

/**
 * Factory function for creating SkillGenerateParams
 */
export const createSkillGenerateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // ID of the SkillEntity to generate code for
    skillId: string;
    // Override output directory (default: persona workspace or commands/)
    outputDir?: string;
  }
): SkillGenerateParams => createPayload(context, sessionId, {
  outputDir: data.outputDir ?? '',
  ...data
});

/**
 * Skill Generate Command Result
 */
export interface SkillGenerateResult extends CommandResult {
  success: boolean;
  // ID of the SkillEntity
  skillId: string;
  // Skill command name
  name: string;
  // Lifecycle status after generation
  status: string;
  // Directory where files were generated
  outputDir: string;
  // Array of generated file paths
  generatedFiles: string[];
  // Human-readable result message
  message: string;
  error?: JTAGError;
}

/**
 * Factory function for creating SkillGenerateResult with defaults
 */
export const createSkillGenerateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // ID of the SkillEntity
    skillId?: string;
    // Skill command name
    name?: string;
    // Lifecycle status after generation
    status?: string;
    // Directory where files were generated
    outputDir?: string;
    // Array of generated file paths
    generatedFiles?: string[];
    // Human-readable result message
    message?: string;
    error?: JTAGError;
  }
): SkillGenerateResult => createPayload(context, sessionId, {
  skillId: data.skillId ?? '',
  name: data.name ?? '',
  status: data.status ?? '',
  outputDir: data.outputDir ?? '',
  generatedFiles: data.generatedFiles ?? [],
  message: data.message ?? '',
  ...data
});

/**
 * Smart Skill Generate-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSkillGenerateResultFromParams = (
  params: SkillGenerateParams,
  differences: Omit<SkillGenerateResult, 'context' | 'sessionId'>
): SkillGenerateResult => transformPayload(params, differences);

/**
 * Skill Generate — Type-safe command executor
 *
 * Usage:
 *   import { SkillGenerate } from '...shared/SkillGenerateTypes';
 *   const result = await SkillGenerate.execute({ ... });
 */
export const SkillGenerate = {
  execute(params: CommandInput<SkillGenerateParams>): Promise<SkillGenerateResult> {
    return Commands.execute<SkillGenerateParams, SkillGenerateResult>('skill/generate', params as Partial<SkillGenerateParams>);
  },
  commandName: 'skill/generate' as const,
} as const;
