/**
 * State Create Command - Shared Types
 *
 * Follows data daemon command pattern for elegant entity state management
 */

import type { JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

export interface StateCreateParams<T extends BaseEntity = BaseEntity> extends JTAGPayload {
  readonly collection: string;
  readonly data: Partial<T>;
  readonly id?: UUID;
  readonly userId?: UUID; // Auto-inject current user context
}

export interface StateCreateResult<T extends BaseEntity> extends JTAGPayload {
  readonly success: boolean;
  readonly item?: T;
  readonly id?: UUID;
  readonly collection: string;
  readonly version?: number;
  readonly error?: string;
}

export function createStateCreateResult<T extends BaseEntity>(
  params: StateCreateParams<T>,
  overrides: Partial<StateCreateResult<T>>
): StateCreateResult<T> {
  return transformPayload(params, {
    success: false,
    collection: params.collection,
    ...overrides
  });
}