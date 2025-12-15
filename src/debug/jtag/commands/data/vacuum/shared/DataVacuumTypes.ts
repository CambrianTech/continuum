/**
 * Data Vacuum Command - Shared Types
 *
 * Reclaim disk space after bulk deletes (VACUUM for SQLite, OPTIMIZE for MySQL, etc.)
 */

import type { CommandParams, CommandResult, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

/**
 * Data Vacuum Command Parameters
 */
export interface DataVacuumParams extends CommandParams {
  // Database handle to vacuum (defaults to primary database)
  dbHandle?: string;
}

/**
 * Factory function for creating DataVacuumParams
 */
export const createDataVacuumParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Database handle to vacuum (defaults to primary database)
    dbHandle?: string;
  }
): DataVacuumParams => createPayload(context, sessionId, {
  dbHandle: data.dbHandle ?? '',
  ...data
});

/**
 * Data Vacuum Command Result
 */
export interface DataVacuumResult extends CommandResult {
  // Whether vacuum completed successfully
  success: boolean;
  // Database handle that was vacuumed
  dbHandle: string;
  // Database size before vacuum (bytes)
  beforeSize: number;
  // Database size after vacuum (bytes)
  afterSize: number;
  // Duration of vacuum operation (ms)
  duration: number;
  // When vacuum completed
  timestamp: string;
  error?: JTAGError;
}

/**
 * Factory function for creating DataVacuumResult with defaults
 */
export const createDataVacuumResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Whether vacuum completed successfully
    success: boolean;
    // Database handle that was vacuumed
    dbHandle?: string;
    // Database size before vacuum (bytes)
    beforeSize?: number;
    // Database size after vacuum (bytes)
    afterSize?: number;
    // Duration of vacuum operation (ms)
    duration?: number;
    // When vacuum completed
    timestamp?: string;
    error?: JTAGError;
  }
): DataVacuumResult => createPayload(context, sessionId, {
  success: data.success ?? false,
  dbHandle: data.dbHandle ?? '',
  beforeSize: data.beforeSize ?? 0,
  afterSize: data.afterSize ?? 0,
  duration: data.duration ?? 0,
  timestamp: data.timestamp ?? '',
  error: data.error
});

/**
 * Smart Data Vacuum-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDataVacuumResultFromParams = (
  params: DataVacuumParams,
  differences: Omit<DataVacuumResult, 'context' | 'sessionId'>
): DataVacuumResult => transformPayload(params, differences);
