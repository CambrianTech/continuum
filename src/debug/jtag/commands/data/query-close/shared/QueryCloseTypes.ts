/**
 * Data Query Close Command Types
 *
 * Closes a query handle and frees resources
 * Should be called when done with pagination
 */

import type { CommandParams, JTAGPayload, JTAGEnvironment } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface DataQueryCloseParams extends CommandParams {
  readonly queryHandle: UUID;
  readonly collection: string; // Required by BaseDataParams
  readonly backend: JTAGEnvironment; // Required by BaseDataParams
}

export interface DataQueryCloseResult extends JTAGPayload {
  readonly success: boolean;
  readonly error?: string;
  readonly timestamp: string; // Required by BaseDataResult
}
