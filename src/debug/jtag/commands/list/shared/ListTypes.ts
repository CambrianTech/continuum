/**
 * List Command Types - Command Discovery Interface
 * 
 * Provides strongly-typed interface for discovering available commands from the system.
 * Essential command that all JTAG systems must implement for client discovery.
 */

import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * List command parameters
 */
export interface ListParams extends JTAGPayload {
  readonly context: JTAGContext;
  readonly sessionId: UUID;

  // Optional filters for command discovery
  readonly includeDescription?: boolean;
  readonly includeSignature?: boolean;
}

/**
 * Command signature information
 */
export interface CommandSignature {
  readonly name: string;
  readonly description: string;
  readonly params?: Record<string, {
    readonly type: string;
    readonly required: boolean;
    readonly description?: string;
  }>;
  readonly returns?: Record<string, {
    readonly type: string;
    readonly description?: string;
  }>;
}

/**
 * List command result
 */
export interface ListResult extends JTAGPayload {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  readonly success: boolean;
  readonly commands: ReadonlyArray<CommandSignature>;
  readonly totalCount: number;
  readonly error?: string;
}

/**
 * Create ListParams with defaults
 */
export function createListParams(
  context: JTAGContext,
  sessionId: UUID,
  overrides: Partial<Omit<ListParams, 'context' | 'sessionId'>> = {}
): ListParams {
  return {
    context,
    sessionId,
    includeDescription: true,
    includeSignature: true,
    ...overrides
  };
}

/**
 * Create ListResult with defaults
 */
export function createListResult(
  context: JTAGContext,
  sessionId: UUID,
  overrides: Partial<Omit<ListResult, 'context' | 'sessionId'>>
): ListResult {
  return {
    context,
    sessionId,
    success: true,
    commands: [],
    totalCount: 0,
    ...overrides
  };
}

/**
 * Create ListResult from params (maintains context/sessionId)
 */
export function createListResultFromParams(
  params: ListParams,
  overrides: Partial<Omit<ListResult, 'context' | 'sessionId'>>
): ListResult {
  return createListResult(params.context, params.sessionId, overrides);
}