/**
 * State Update Types - User-aware entity updates
 *
 * Provides elegant wrapper around data/update with automatic user context injection
 * Following the established state command pattern for simple delegation
 */

import type { JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export interface StateUpdateParams<T extends BaseEntity = BaseEntity> extends JTAGPayload {
  readonly collection: string;
  readonly id: UUID;
  readonly data: Partial<T>;
  readonly userId?: UUID; // Auto-inject current user context
}

export interface StateUpdateResult<T extends BaseEntity> extends JTAGPayload {
  readonly success: boolean;
  readonly item?: T;
  readonly id?: UUID;
  readonly collection: string;
  readonly version?: number;
  readonly error?: string;
}

export function createStateUpdateResult<T extends BaseEntity>(
  context: any,
  sessionId: string,
  overrides: Partial<StateUpdateResult<T>>
): StateUpdateResult<T> {
  return {
    context,
    sessionId,
    success: false,
    collection: '',
    ...overrides
  };
}