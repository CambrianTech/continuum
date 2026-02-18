/**
 * Data Command Constants
 *
 * All data/* command names defined here.
 * Usage:
 *   await Commands.execute(DATA_COMMANDS.LIST, params);
 */

export const DATA_COMMANDS = {
  /** List entities from a collection */
  LIST: 'data/list',

  /** Create a new entity */
  CREATE: 'data/create',

  /** Read a single entity by ID */
  READ: 'data/read',

  /** Update an existing entity */
  UPDATE: 'data/update',

  /** Delete an entity by ID */
  DELETE: 'data/delete',

  /** Truncate all records in a collection */
  TRUNCATE: 'data/truncate',

  /** Clear all data from all collections */
  CLEAR: 'data/clear',

  /** Get collection schema information */
  SCHEMA: 'data/schema',

  /** Open a database handle */
  OPEN: 'data/open',

  /** Close a database handle */
  CLOSE: 'data/close',

  /** List all database handles */
  LIST_HANDLES: 'data/list-handles',
} as const;

/**
 * Type-safe data command names
 */
export type DataCommand = typeof DATA_COMMANDS[keyof typeof DATA_COMMANDS];
