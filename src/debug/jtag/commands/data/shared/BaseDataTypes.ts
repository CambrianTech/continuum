/**
 * Base Data Command Types - Generic Abstractions
 *
 * Following ARCHITECTURE-RULES.md:
 * ✅ Generic programming with BaseEntity only
 * ✅ No specific entity references (UserEntity, etc.)
 * ✅ Backend routing abstraction
 */

import type { JTAGPayload, JTAGContext, JTAGEnvironment } from '../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * Base interface for all data command parameters
 * Uses JTAGEnvironment for routing capability
 */
export interface BaseDataParams extends JTAGPayload {
  readonly collection: string;
  readonly backend: JTAGEnvironment;
}

/**
 * Base interface for all data command results
 */
export interface BaseDataResult extends JTAGPayload {
  readonly success: boolean;
  readonly error?: string;
  readonly timestamp: string;
}

/**
 * Backend routing type for elegant environment selection
 */
export type DataBackend = 'server' | 'local';

/**
 * Factory function for creating base data params
 */
export const createBaseDataParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<BaseDataParams, 'context' | 'sessionId' | 'backend'> & { backend?: JTAGEnvironment }
): BaseDataParams => createPayload(context, sessionId, {
  ...data,
  backend: data.backend ?? 'server'
});

/**
 * Transform params to result with proper timestamp
 */
export const createBaseDataResultFromParams = (
  params: BaseDataParams,
  differences: Omit<Partial<BaseDataResult>, 'context' | 'sessionId'>
): BaseDataResult => transformPayload(params, {
  success: false,
  timestamp: new Date().toISOString(),
  ...differences
});