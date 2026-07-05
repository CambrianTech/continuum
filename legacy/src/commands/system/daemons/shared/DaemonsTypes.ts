/**
 * Daemons Command Types
 *
 * List all registered system daemons with their status
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface DaemonsParams extends CommandParams {
  // Optional filters
  nameFilter?: string;      // Filter by daemon name (partial match)
  statusOnly?: boolean;     // Only return count/status, not full details
}

export interface DaemonInfo {
  name: string;
  status: 'active' | 'inactive';
  type?: string;            // Optional daemon type/category
}

export interface DaemonsResult extends CommandResult {
  success: boolean;
  daemons: DaemonInfo[];
  total: number;
  active: number;
  error?: string;
}

/**
 * Daemons — Type-safe command executor
 *
 * Usage:
 *   import { Daemons } from '...shared/DaemonsTypes';
 *   const result = await Daemons.execute({ ... });
 */
export const Daemons = {
  execute(params: CommandInput<DaemonsParams>): Promise<DaemonsResult> {
    return Commands.execute<DaemonsParams, DaemonsResult>('system/daemons', params as Partial<DaemonsParams>);
  },
  commandName: 'system/daemons' as const,
} as const;

/**
 * Factory function for creating SystemDaemonsParams
 */
export const createSystemDaemonsParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DaemonsParams, 'context' | 'sessionId' | 'userId'>
): DaemonsParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating SystemDaemonsResult with defaults
 */
export const createSystemDaemonsResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DaemonsResult, 'context' | 'sessionId' | 'userId'>
): DaemonsResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart system/daemons-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createSystemDaemonsResultFromParams = (
  params: DaemonsParams,
  differences: Omit<DaemonsResult, 'context' | 'sessionId' | 'userId'>
): DaemonsResult => transformPayload(params, differences);

