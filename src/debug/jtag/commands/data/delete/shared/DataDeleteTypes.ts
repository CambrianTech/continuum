/**
 * Data Delete Command Types - Universal Delete Interface
 * 
 * Follows working data create pattern with strong typing
 */

import type { CommandParams, JTAGPayload, JTAGContext, CommandInput } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DbHandle } from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';

/**
 * Data Delete Parameters
 */
export interface DataDeleteParams extends CommandParams {
  readonly collection: string;
  readonly id: UUID;
  readonly format?: 'json' | 'yaml' | 'table';
  /** Optional database handle for multi-database operations (defaults to 'default') */
  readonly dbHandle?: DbHandle;
  /** Suppress CRUD events for this operation (for internal operations like archiving) */
  readonly suppressEvents?: boolean;
}

/**
 * Data Delete Result
 */
export interface DataDeleteResult extends JTAGPayload {
  readonly collection: string;
  readonly id: UUID;
  readonly found: boolean;
  readonly deleted: boolean;
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * Factory function for creating DataDeleteParams
 */
export const createDataDeleteParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataDeleteParams, 'context' | 'sessionId'>
): DataDeleteParams => createPayload(context, sessionId, data);

/**
 * Transform params to result
 */
export const createDataDeleteResultFromParams = (
  params: DataDeleteParams,
  differences: Omit<Partial<DataDeleteResult>, 'context' | 'sessionId'>
): DataDeleteResult => transformPayload(params, {
  collection: params.collection,
  id: params.id,
  found: false,
  deleted: false,
  timestamp: new Date().toISOString(),
  ...differences
});

/**
 * DataDelete — Type-safe command executor
 *
 * Usage:
 *   import { DataDelete } from '@commands/data/delete/shared/DataDeleteTypes';
 *   const result = await DataDelete.execute({ collection: 'users', id: userId });
 */
export const DataDelete = {
  execute(params: CommandInput<DataDeleteParams>): Promise<DataDeleteResult> {
    return Commands.execute<DataDeleteParams, DataDeleteResult>('data/delete', params as Partial<DataDeleteParams>);
  },
  commandName: 'data/delete' as const,
} as const;