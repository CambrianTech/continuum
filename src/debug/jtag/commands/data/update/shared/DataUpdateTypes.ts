/**
 * Data Update Command Types - Universal Update Interface
 * 
 * Follows working data create pattern with strong typing
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

/**
 * Data Update Parameters
 */
export interface DataUpdateParams extends JTAGPayload {
  readonly collection: string;
  readonly id: UUID;
  readonly data: Record<string, unknown>;
  readonly format?: 'json' | 'yaml' | 'table';
  readonly incrementVersion?: boolean;
}

/**
 * Data Update Result
 */
export interface DataUpdateResult extends JTAGPayload {
  readonly collection: string;
  readonly id: UUID;
  readonly found: boolean;
  readonly data?: Record<string, unknown>;
  readonly previousVersion?: number;
  readonly newVersion?: number;
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * Factory function for creating DataUpdateParams
 */
export const createDataUpdateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataUpdateParams, 'context' | 'sessionId'>
): DataUpdateParams => createPayload(context, sessionId, data);

/**
 * Transform params to result
 */
export const createDataUpdateResultFromParams = (
  params: DataUpdateParams,
  differences: Omit<Partial<DataUpdateResult>, 'context' | 'sessionId'>
): DataUpdateResult => transformPayload(params, {
  collection: params.collection,
  id: params.id,
  found: false,
  timestamp: new Date().toISOString(),
  ...differences
});