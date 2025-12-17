/**
 * Data Query Open Command Types
 *
 * Opens a paginated query and returns a handle (UUID)
 * DataDaemon maintains the query state (filters, sorting, cursor position)
 */

import type { CommandParams, JTAGPayload, JTAGEnvironment } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface DataQueryOpenParams extends CommandParams {
  readonly collection: string;
  readonly backend: JTAGEnvironment;
  readonly filter?: Record<string, any>;
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  readonly pageSize?: number;
}

export interface DataQueryOpenResult extends JTAGPayload {
  readonly success: boolean;
  readonly queryHandle: UUID; // Opaque handle for subsequent nextPage calls
  readonly totalCount: number;
  readonly pageSize: number;
  readonly error?: string;
  readonly timestamp: string; // Required by BaseDataResult
}
