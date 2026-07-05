/**
 * Data Open Command - ADVANCED: Opens secondary database handles
 *
 * WARNING: Most commands use the default database automatically.
 * You probably want data/list or data/read instead of this command.
 *
 * Only use data/open when you need to access a DIFFERENT database file.
 *
 * Required params:
 * - adapter: MUST be 'sqlite', 'json', 'vector', 'graph', or 'rust'
 * - config: { path: "/path/to/database" } for sqlite/json
 *
 * @example data/open --adapter="sqlite" --config='{"path":"/tmp/other.db"}'
 */

import type { CommandParams, JTAGPayload, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '../../../../system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
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
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Data Open Parameters
 *
 * @description Opens a new database handle. Most commands use the default database
 * automatically - you only need this for multi-database scenarios.
 *
 * Valid adapter types: 'sqlite', 'json', 'vector', 'graph', 'rust'
 * - sqlite: SQLite database file (most common)
 * - json: JSON file storage
 * - vector: Vector database (Qdrant, Pinecone)
 * - graph: Graph database (Neo4j)
 * - rust: Rust worker storage
 */
export interface DataOpenParams extends CommandParams {
  /**
   * Adapter type - MUST be one of: 'sqlite', 'json', 'vector', 'graph', 'rust'
   * @example "sqlite"
   */
  readonly adapter: AdapterType;

  /**
   * Adapter-specific configuration object.
   * For sqlite: { path: "/path/to/db.sqlite" }
   * For json: { path: "/path/to/file.json" }
   */
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

/**
 * DataOpen — Type-safe command executor
 *
 * Usage:
 *   import { DataOpen } from '...shared/DataOpenTypes';
 *   const result = await DataOpen.execute({ ... });
 */
export const DataOpen = {
  execute(params: CommandInput<DataOpenParams>): Promise<DataOpenResult> {
    return Commands.execute<DataOpenParams, DataOpenResult>('data/open', params as Partial<DataOpenParams>);
  },
  commandName: 'data/open' as const,
} as const;
