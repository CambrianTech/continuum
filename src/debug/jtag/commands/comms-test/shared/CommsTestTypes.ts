/**
 * Comms Test Command Types - Database Testing Edition
 */

import type { CommandParams, JTAGPayload, CommandInput} from '../../../system/core/types/JTAGTypes';
import { Commands } from '../../../system/core/shared/Commands';

export interface CommsTestParams extends CommandParams {
  // Test mode
  mode: 'echo' | 'database';

  // Echo mode
  message?: string;

  // Database mode
  dbCount?: number;           // Number of concurrent databases to test (default: 5)
  testDir?: string;           // Test directory for databases (default: .continuum/jtag/test-dbs)
  operations?: number;        // Number of operations per database (default: 10)
}

export interface CommsTestResult extends JTAGPayload {
  success: boolean;

  // Echo mode results
  echo?: string;

  // Database mode results
  databases?: {
    handle: string;
    path: string;
    operations: number;
    duration: number;
    success: boolean;
    error?: string;
  }[];

  totalDuration?: number;
  totalOperations?: number;
}

/**
 * CommsTest — Type-safe command executor
 *
 * Usage:
 *   import { CommsTest } from '...shared/CommsTestTypes';
 *   const result = await CommsTest.execute({ ... });
 */
export const CommsTest = {
  execute(params: CommandInput<CommsTestParams>): Promise<CommsTestResult> {
    return Commands.execute<CommsTestParams, CommsTestResult>('comms-test', params as Partial<CommsTestParams>);
  },
  commandName: 'comms-test' as const,
} as const;
