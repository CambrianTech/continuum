/**
 * Data List Command - Shared Types
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface DataListParams extends JTAGPayload {
  readonly collection: string;
  readonly limit?: number;
}

export interface DataListResult extends JTAGPayload {
  readonly success: boolean;
  readonly items: readonly any[];
  readonly collection: string;
  readonly count: number;
  readonly timestamp: string;
  readonly error?: string;
}

export const createDataListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataListParams, 'context' | 'sessionId'>
): DataListParams => createPayload(context, sessionId, data);

export const createDataListResultFromParams = (
  params: DataListParams,
  differences: Omit<Partial<DataListResult>, 'context' | 'sessionId'>
): DataListResult => transformPayload(params, {
  success: false,
  items: [],
  collection: params.collection,
  count: 0,
  timestamp: new Date().toISOString(),
  ...differences
});