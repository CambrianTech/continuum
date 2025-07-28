/**
 * Test Error Command Types
 * 
 * Simple command that intentionally throws errors for testing error handling flow
 */

import type { CommandParams, CommandResult, JTAGContext } from '@shared/JTAGTypes';
import type { UUID } from '@shared/CrossPlatformUUID';

export interface TestErrorParams extends CommandParams {
  context: JTAGContext;
  sessionId: UUID;
  errorType?: 'generic' | 'validation' | 'network' | 'permission';
  errorMessage?: string;
  shouldThrow?: boolean;
}

export interface TestErrorResult extends CommandResult {
  success: boolean;
  errorType?: string;
  message?: string;
  timestamp: string;
  environment: string;
  context: JTAGContext;
  sessionId: UUID;
}

export function createTestErrorResult(
  context: JTAGContext, 
  sessionId: UUID, 
  result: Partial<TestErrorResult>
): TestErrorResult {
  return {
    success: result.success ?? false,
    errorType: result.errorType,
    message: result.message,
    timestamp: new Date().toISOString(),
    environment: context.environment,
    context,
    sessionId,
    ...result
  };
}