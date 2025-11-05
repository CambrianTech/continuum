/**
 * State Get Command - Shared Types
 *
 * Follows data daemon command pattern for elegant entity state management
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export interface StateGetParams<T extends BaseEntity = BaseEntity> extends JTAGPayload {
  readonly collection: string;
  readonly limit?: number;
  readonly filter?: Record<string, any>;
  readonly orderBy?: { field: string; direction: 'asc' | 'desc' }[];
  readonly userId?: UUID; // Auto-inject current user context
}

export interface StateGetResult<T extends BaseEntity> extends JTAGPayload {
  readonly success: boolean;
  readonly items: readonly T[];
  readonly collection: string;
  readonly count: number;
  readonly timestamp: string;
  readonly error?: string;
}

export const createStateGetParams = <T extends BaseEntity>(
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<StateGetParams<T>, 'context' | 'sessionId'>
): StateGetParams<T> => createPayload(context, sessionId, data);

export const createStateGetResult = <T extends BaseEntity>(
  params: StateGetParams<T>,
  differences: Omit<Partial<StateGetResult<T>>, 'context' | 'sessionId'>
): StateGetResult<T> => transformPayload(params, {
  success: false,
  items: [],
  collection: params.collection,
  count: 0,
  timestamp: new Date().toISOString(),
  ...differences
});