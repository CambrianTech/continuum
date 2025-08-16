/**
 * Data Read Command - Shared Types
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface DataReadParams extends JTAGPayload {
  readonly collection: string;
  readonly id: UUID;
}

export interface DataReadResult extends JTAGPayload {
  readonly success: boolean;
  readonly data?: any;
  readonly found: boolean;
  readonly collection: string;
  readonly id: UUID;
  readonly timestamp: string;
  readonly error?: string;
}

export const createDataReadParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataReadParams, 'context' | 'sessionId'>
): DataReadParams => createPayload(context, sessionId, data);

export const createDataReadResultFromParams = (
  params: DataReadParams,
  differences: Omit<Partial<DataReadResult>, 'context' | 'sessionId'>
): DataReadResult => transformPayload(params, {
  success: false,
  found: false,
  collection: params.collection,
  id: params.id,
  timestamp: new Date().toISOString(),
  ...differences
});