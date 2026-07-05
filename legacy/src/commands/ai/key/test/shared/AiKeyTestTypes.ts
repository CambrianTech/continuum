/**
 * Ai Key Test Command - Shared Types
 *
 * Test an API key before saving it. Makes a minimal API call to verify the key is valid and has sufficient permissions.
 */

import type { JTAGContext, CommandInput, CommandParams } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';
import {
  type AiKeyParams,
  type AiKeyResult,
  createAiKeyParams,
  createAiKeyResult
} from '../../common/AiKeyBase';

/**
 * Ai Key Test Command Parameters
 */
export interface AiKeyTestParams extends CommandParams, AiKeyParams {
  // Provider to test (anthropic, openai, groq, deepseek, xai, together, fireworks)
  provider: string;
  // API key to test (will NOT be stored)
  key: string;
  // If true, use the stored key from config.env instead of the provided key
  useStored?: boolean;
}

/**
 * Factory function for creating AiKeyTestParams
 */
export const createAiKeyTestParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Provider to test (anthropic, openai, groq, deepseek, xai, together, fireworks)
    provider: string;
    // API key to test (will NOT be stored)
    key: string;
    useStored?: boolean;
  }
): AiKeyTestParams => createAiKeyParams(context, sessionId, {
  ...data
});

/**
 * Ai Key Test Command Result
 */
export interface AiKeyTestResult extends AiKeyResult {
  // Whether the key is valid
  valid: boolean;
  // Provider that was tested
  provider: string;
  // Response time in milliseconds
  responseTimeMs: number;
  // Error message if key is invalid (optional)
  errorMessage?: string;
  // Available models for this key (optional)
  models?: string[];
}

/**
 * Factory function for creating AiKeyTestResult with defaults
 */
export const createAiKeyTestResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    valid?: boolean;
    provider?: string;
    responseTimeMs?: number;
    errorMessage?: string;
    models?: string[];
  }
): AiKeyTestResult => createAiKeyResult(context, sessionId, {
  valid: data.valid ?? false,
  provider: data.provider ?? '',
  responseTimeMs: data.responseTimeMs ?? 0,
  ...data
});

/**
 * Smart Ai Key Test-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createAiKeyTestResultFromParams = (
  params: AiKeyTestParams,
  differences: Omit<AiKeyTestResult, 'context' | 'sessionId'>
): AiKeyTestResult => transformPayload(params, differences);

/**
 * AiKeyTest — Type-safe command executor
 *
 * Usage:
 *   import { AiKeyTest } from '...shared/AiKeyTestTypes';
 *   const result = await AiKeyTest.execute({ ... });
 */
export const AiKeyTest = {
  execute(params: CommandInput<AiKeyTestParams>): Promise<AiKeyTestResult> {
    return Commands.execute<AiKeyTestParams, AiKeyTestResult>('ai/key/test', params as Partial<AiKeyTestParams>);
  },
  commandName: 'ai/key/test' as const,
} as const;
