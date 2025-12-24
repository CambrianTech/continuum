/**
 * Comms Test Command Types - Database Testing Edition
 */

import type { CommandParams, JTAGPayload } from '../../../system/core/types/JTAGTypes';

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
