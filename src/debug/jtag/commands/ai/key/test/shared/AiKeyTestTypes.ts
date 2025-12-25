/**
 * Ai Key Test Command - Shared Types
 *
 * Test an API key before saving it. Makes a minimal API call to verify the key is valid and has sufficient permissions.
 */

import type { CommandParams, CommandResult, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Ai Key Test Command Parameters
 */
export interface AiKeyTestParams extends CommandParams {
  // Provider to test (anthropic, openai, groq, deepseek, xai, together, fireworks)
  provider: string;
  // API key to test (will NOT be stored)
  key: string;
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
  }
): AiKeyTestParams => createPayload(context, sessionId, {

  ...data
});

/**
 * Ai Key Test Command Result
 */
export interface AiKeyTestResult extends CommandResult {
  success: boolean;
  // Whether the key is valid
  valid: boolean;
  // Provider that was tested
  provider: string;
  // Response time in milliseconds
  responseTime: number;
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
    responseTime?: number;
    errorMessage?: string;
    models?: string[];
  }
): AiKeyTestResult => createPayload(context, sessionId, {
  valid: data.valid ?? false,
  provider: data.provider ?? '',
  responseTime: data.responseTime ?? 0,
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
