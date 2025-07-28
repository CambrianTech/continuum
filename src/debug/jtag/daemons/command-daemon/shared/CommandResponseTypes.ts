/**
 * Command Daemon Response Types
 * 
 * Response types and factories specific to command daemon operations.
 * Co-located with command daemon to maintain proper module boundaries.
 */

import { type JTAGContext, createPayload } from '@shared/JTAGTypes';
import { type BaseResponsePayload } from '@shared/ResponseTypes';
import { UUID } from 'crypto';

// Command daemon response types
export interface CommandSuccessResponse extends BaseResponsePayload {
  commandResult: unknown; // Specific command results can extend this
  executionTime?: number;
}

export interface CommandErrorResponse extends BaseResponsePayload {
  error: string;
  commandName?: string;
}

export const createCommandSuccessResponse = (
  commandResult: unknown,
  context: JTAGContext,
  executionTime: number | undefined,
  sessionId: UUID
): CommandSuccessResponse => createPayload(context, sessionId, {
  success: true,
  timestamp: new Date().toISOString(),
  commandResult,
  executionTime
});

export const createCommandErrorResponse = (
  error: string,
  context: JTAGContext,
  commandName: string | undefined,
  sessionId: UUID
): CommandErrorResponse => createPayload(context, sessionId, {
  success: false,
  timestamp: new Date().toISOString(),
  error,
  commandName
});

// Union type for command daemon responses
export type CommandResponse = CommandSuccessResponse | CommandErrorResponse;