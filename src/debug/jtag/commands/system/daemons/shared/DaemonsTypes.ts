/**
 * Daemons Command Types
 *
 * List all registered system daemons with their status
 */

import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';

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
