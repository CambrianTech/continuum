/**
 * Daemons Command Types
 *
 * List all registered system daemons with their status
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';

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
