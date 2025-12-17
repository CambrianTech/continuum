/**
 * Data Open Command - Shared Types
 *
 * Opens a new database handle for multi-database operations.
 * Storage-adapter-agnostic: works with SQLite, JSON, Vector DB, Graph DB, etc.
 *
 * See docs/MULTI-DATABASE-HANDLES.md for architecture
 */

import type { CommandParams, JTAGPayload, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type {
  DbHandle,
  AdapterType,
  AdapterConfig,
  SqliteConfig,
  JsonConfig,
  VectorConfig,
  GraphConfig
} from '../../../../daemons/data-daemon/server/DatabaseHandleRegistry';

/**
 * Data Open Parameters
 */
export interface DataOpenParams extends CommandParams {
  // Adapter type: 'sqlite', 'json', 'vector', 'graph'
  readonly adapter: AdapterType;

  // Adapter-specific configuration
  // Type depends on adapter (SqliteConfig, JsonConfig, etc.)
  readonly config: AdapterConfig;
}

/**
 * Data Open Result
 */
export interface DataOpenResult extends JTAGPayload {
  readonly success: boolean;
  readonly dbHandle?: DbHandle;  // Opaque identifier for this database connection
  readonly adapter?: AdapterType;
  readonly timestamp: string;
  readonly error?: string;
}

/**
 * Factory function for creating data/open params
 */
export const createDataOpenParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DataOpenParams, 'context' | 'sessionId'>
): DataOpenParams => createPayload(context, sessionId, data);

/**
 * Transform params to result
 */
export const createDataOpenResultFromParams = (
  params: DataOpenParams,
  differences: Omit<Partial<DataOpenResult>, 'context' | 'sessionId'>
): DataOpenResult => transformPayload(params, {
  success: false,
  timestamp: new Date().toISOString(),
  ...differences
});

// Re-export types from DatabaseHandleRegistry for convenience
export type {
  DbHandle,
  AdapterType,
  AdapterConfig,
  SqliteConfig,
  JsonConfig,
  VectorConfig,
  GraphConfig
};
