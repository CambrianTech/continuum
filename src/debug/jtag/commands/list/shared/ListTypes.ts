/**
 * List Command Types - Command Discovery Interface
 * 
 * Provides strongly-typed interface for discovering available commands from the system.
 * Essential command that all JTAG systems must implement for client discovery.
 */

import type { JTAGContext, CommandParams, JTAGPayload, CommandInput} from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { SYSTEM_SCOPES } from '../../../system/core/types/SystemScopes';
import { Commands } from '../../../system/core/shared/Commands';

/**
 * List command parameters
 */
export interface ListParams extends CommandParams {
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
    userId: SYSTEM_SCOPES.SYSTEM,
    includeDescription: false,  // Compact by default - use help <cmd> for details
    includeSignature: false,    // Compact by default - use help <cmd> for details
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
/**
 * List — Type-safe command executor
 *
 * Usage:
 *   import { List } from '...shared/ListTypes';
 *   const result = await List.execute({ ... });
 */
export const List = {
  execute(params: CommandInput<ListParams>): Promise<ListResult> {
    return Commands.execute<ListParams, ListResult>('list', params as Partial<ListParams>);
  },
  commandName: 'list' as const,
} as const;
