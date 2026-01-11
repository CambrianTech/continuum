/**
 * State Create Command - Shared Types
 *
 * Follows data daemon command pattern for elegant entity state management
 */

import type { JTAGPayload, JTAGContext, CommandParams } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';

/** State create command parameters */
export interface StateCreateParams extends CommandParams {
  /** Collection to create entity in */
  readonly collection: string;
  /** Entity data to create */
  readonly data: Record<string, any>;
  /** Optional explicit ID */
  readonly id?: UUID;
  /** User ID for context */
  readonly userId?: UUID;
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
  params: StateCreateParams,
  overrides: Partial<StateCreateResult<T>>
): StateCreateResult<T> {
  return transformPayload(params, {
    success: false,
    collection: params.collection,
    ...overrides
  });
}