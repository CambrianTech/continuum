/**
 * Storage Adapter Interface
 *
 * Backend-agnostic storage abstraction that can be implemented
 * for SQLite, JSON files, MongoDB, or any other storage system.
 */

export interface FilterCondition {
  [key: string]: any;
}

export interface OrderBy {
  field: string;
  direction: 'asc' | 'desc';
}

export interface StorageQuery {
  collection: string;
  filter?: FilterCondition;
  orderBy?: OrderBy[];
  limit?: number;
}

export interface StorageResult<T> {
  success: boolean;
  items: T[];
  count: number;
  error?: string;
}

export interface StorageAdapter {
  /**
   * Query items from storage
   */
  query<T>(query: StorageQuery): Promise<StorageResult<T>>;

  /**
   * Store an item
   */
  store<T>(collection: string, id: string, data: T): Promise<boolean>;

  /**
   * Delete an item
   */
  delete(collection: string, id: string): Promise<boolean>;

  /**
   * Check if storage is available
   */
  isAvailable(): Promise<boolean>;
}