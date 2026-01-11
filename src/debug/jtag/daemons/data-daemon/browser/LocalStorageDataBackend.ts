/**
 * LocalStorageDataBackend - localStorage integration with data daemon
 *
 * Allows localStorage to be used as a storage backend through the existing data commands:
 * - data/create -> localStorage.setItem()
 * - data/read -> localStorage.getItem()
 * - data/update -> localStorage.setItem() with merge
 * - data/delete -> localStorage.removeItem()
 * - data/list -> localStorage key enumeration
 *
 * Uses AsyncStorage for non-blocking writes (debounced, requestIdleCallback).
 * Reads are synchronous for read-your-writes consistency but check pending queue first.
 *
 * Benefits:
 * - Reuses existing entity validation and types
 * - Consistent data command interface
 * - Automatic entity serialization/deserialization
 * - Type safety through existing entity system
 * - Non-blocking writes via asyncStorage
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';
import { asyncStorage } from '../../../system/core/browser/AsyncStorage';

export interface LocalStorageEntityData {
  id: UUID;
  collection: string;
  entity: any; // The actual entity data
  version: number;
  createdAt: string;
  updatedAt: string;
  metadata?: Record<string, unknown>;
}

export class LocalStorageDataBackend {
  private static readonly VERSION = 1;
  private static readonly KEY_PREFIX = 'continuum-entity-';
  private static readonly INDEX_KEY = 'continuum-entity-index';

  /**
   * Get storage key for a specific entity
   */
  private static getEntityKey(collection: string, id: UUID): string {
    return `${this.KEY_PREFIX}${collection}:${id}`;
  }

  /**
   * Get all entity keys for a collection
   * Uses asyncStorage.keys() which includes pending writes
   */
  private static getCollectionKeys(collection?: string): string[] {
    const allKeys = asyncStorage.keys();
    const keys: string[] = [];

    for (const key of allKeys) {
      if (key && key.startsWith(this.KEY_PREFIX)) {
        if (!collection) {
          keys.push(key);
        } else {
          const keyCollection = key.replace(this.KEY_PREFIX, '').split(':')[0];
          if (keyCollection === collection) {
            keys.push(key);
          }
        }
      }
    }

    return keys;
  }

  /**
   * Create entity in localStorage (equivalent to data/create)
   */
  static async create<T extends BaseEntity>(
    collection: string,
    entity: T
  ): Promise<{ success: boolean; id?: UUID; error?: string }> {
    try {
      if (!entity.id) {
        return { success: false, error: 'Entity ID is required for localStorage create' };
      }

      // Validate entity using its built-in validation if available
      if (typeof entity.validate === 'function') {
        const validation = entity.validate();
        if (!validation.success) {
          return { success: false, error: validation.error };
        }
      }

      const key = this.getEntityKey(collection, entity.id);

      // Check if entity already exists (asyncStorage checks pending writes first)
      if (asyncStorage.getItem(key)) {
        return { success: false, error: `Entity ${entity.id} already exists in collection ${collection}` };
      }

      const entityData: LocalStorageEntityData = {
        id: entity.id,
        collection,
        entity: entity,
        version: this.VERSION,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString()
      };

      // Non-blocking write via asyncStorage
      asyncStorage.setItem(key, JSON.stringify(entityData));
      this.updateIndex(collection, entity.id, 'create');

      // Silent - no logging in hot path
      return { success: true, id: entity.id };

    } catch (error) {
      console.error('❌ LocalStorageDataBackend: Create failed:', error);
      return { success: false, error: String(error) };
    }
  }

  /**
   * Read entity from localStorage (equivalent to data/read)
   * Uses asyncStorage.getItem which checks pending writes first for read-your-writes consistency
   */
  static async read<T extends BaseEntity>(
    collection: string,
    id: UUID
  ): Promise<{ success: boolean; entity?: T; error?: string }> {
    try {
      const key = this.getEntityKey(collection, id);
      const stored = asyncStorage.getItem(key);

      if (!stored) {
        return { success: false, error: `Entity ${id} not found in collection ${collection}` };
      }

      const entityData: LocalStorageEntityData = JSON.parse(stored);

      // Version compatibility check
      if (entityData.version !== this.VERSION) {
        console.warn(`⚠️ LocalStorageDataBackend: Version mismatch for ${collection}:${id}`);
      }

      return { success: true, entity: entityData.entity as T };

    } catch (error) {
      console.error('❌ LocalStorageDataBackend: Read failed:', error);
      return { success: false, error: String(error) };
    }
  }

  /**
   * Update entity in localStorage (equivalent to data/update)
   * Uses asyncStorage for non-blocking writes
   */
  static async update<T extends BaseEntity>(
    collection: string,
    id: UUID,
    updateData: Partial<T>
  ): Promise<{ success: boolean; error?: string }> {
    try {
      const key = this.getEntityKey(collection, id);
      const stored = asyncStorage.getItem(key);

      if (!stored) {
        return { success: false, error: `Entity ${id} not found in collection ${collection}` };
      }

      const entityData: LocalStorageEntityData = JSON.parse(stored);

      // Merge update data with existing entity
      const updatedEntity = { ...entityData.entity, ...updateData };

      // Update metadata
      entityData.entity = updatedEntity;
      entityData.updatedAt = new Date().toISOString();

      // Non-blocking write via asyncStorage
      asyncStorage.setItem(key, JSON.stringify(entityData));

      // Silent - no logging in hot path
      return { success: true };

    } catch (error) {
      console.error('❌ LocalStorageDataBackend: Update failed:', error);
      return { success: false, error: String(error) };
    }
  }

  /**
   * Delete entity from localStorage (equivalent to data/delete)
   * Uses asyncStorage for non-blocking removal
   */
  static async delete(
    collection: string,
    id: UUID
  ): Promise<{ success: boolean; error?: string }> {
    try {
      const key = this.getEntityKey(collection, id);

      if (!asyncStorage.hasItem(key)) {
        return { success: false, error: `Entity ${id} not found in collection ${collection}` };
      }

      // Non-blocking remove via asyncStorage
      asyncStorage.removeItem(key);
      this.updateIndex(collection, id, 'delete');

      // Silent - no logging in hot path
      return { success: true };

    } catch (error) {
      console.error('❌ LocalStorageDataBackend: Delete failed:', error);
      return { success: false, error: String(error) };
    }
  }

  /**
   * List entities from localStorage (equivalent to data/list)
   * Uses asyncStorage.getItem which checks pending writes first
   */
  static async list<T extends BaseEntity>(
    collection: string,
    filter?: Record<string, unknown>
  ): Promise<{ success: boolean; entities?: T[]; count?: number; error?: string }> {
    try {
      const keys = this.getCollectionKeys(collection);
      const entities: T[] = [];

      for (const key of keys) {
        const stored = asyncStorage.getItem(key);
        if (stored) {
          try {
            const entityData: LocalStorageEntityData = JSON.parse(stored);

            // Apply filter if provided
            if (filter) {
              const matchesFilter = Object.entries(filter).every(([filterKey, filterValue]) => {
                return entityData.entity[filterKey] === filterValue;
              });
              if (!matchesFilter) {
                continue;
              }
            }

            entities.push(entityData.entity as T);
          } catch (parseError) {
            console.warn(`⚠️ LocalStorageDataBackend: Could not parse entity from key ${key}:`, parseError);
          }
        }
      }

      return { success: true, entities, count: entities.length };

    } catch (error) {
      console.error('❌ LocalStorageDataBackend: List failed:', error);
      return { success: false, error: String(error) };
    }
  }

  /**
   * Clear all entities from a collection
   * Uses asyncStorage for non-blocking removal
   */
  static async clear(collection: string): Promise<{ success: boolean; error?: string }> {
    try {
      const keys = this.getCollectionKeys(collection);

      for (const key of keys) {
        // Non-blocking remove via asyncStorage
        asyncStorage.removeItem(key);
      }

      // Clear collection from index
      this.clearCollectionFromIndex(collection);

      // Silent - no logging in hot path
      return { success: true };

    } catch (error) {
      console.error('❌ LocalStorageDataBackend: Clear failed:', error);
      return { success: false, error: String(error) };
    }
  }

  /**
   * Maintain index of collections and entities for faster querying
   * Uses asyncStorage for non-blocking index updates
   */
  private static updateIndex(collection: string, id: UUID, operation: 'create' | 'delete'): void {
    try {
      const indexData = JSON.parse(asyncStorage.getItem(this.INDEX_KEY) || '{}');

      if (!indexData[collection]) {
        indexData[collection] = [];
      }

      if (operation === 'create') {
        if (!indexData[collection].includes(id)) {
          indexData[collection].push(id);
        }
      } else if (operation === 'delete') {
        indexData[collection] = indexData[collection].filter((entityId: UUID) => entityId !== id);
        if (indexData[collection].length === 0) {
          delete indexData[collection];
        }
      }

      // Non-blocking index write via asyncStorage
      asyncStorage.setItem(this.INDEX_KEY, JSON.stringify(indexData));

    } catch (error) {
      console.warn('⚠️ LocalStorageDataBackend: Index update failed:', error);
    }
  }

  /**
   * Clear collection from index
   * Uses asyncStorage for non-blocking index updates
   */
  private static clearCollectionFromIndex(collection: string): void {
    try {
      const indexData = JSON.parse(asyncStorage.getItem(this.INDEX_KEY) || '{}');
      delete indexData[collection];
      // Non-blocking index write via asyncStorage
      asyncStorage.setItem(this.INDEX_KEY, JSON.stringify(indexData));
    } catch (error) {
      console.warn('⚠️ LocalStorageDataBackend: Index clear failed:', error);
    }
  }

  /**
   * Get storage statistics
   * Uses asyncStorage.keys() for consistent view including pending writes
   */
  static getStats(): {
    totalKeys: number;
    entityKeys: number;
    collections: string[];
    storageUsed: number
  } {
    const entityKeys = this.getCollectionKeys();
    const collections = [...new Set(
      entityKeys.map(key => key.replace(this.KEY_PREFIX, '').split(':')[0])
    )];

    const allKeys = asyncStorage.keys();
    let storageUsed = 0;
    for (const key of allKeys) {
      const value = asyncStorage.getItem(key);
      if (value) {
        storageUsed += key.length + value.length;
      }
    }

    return {
      totalKeys: allKeys.length,
      entityKeys: entityKeys.length,
      collections,
      storageUsed
    };
  }

  /**
   * Debug helper
   * Uses asyncStorage for consistent view
   */
  static debug(): void {
    const stats = this.getStats();
    console.group('🔧 LocalStorageDataBackend Debug');
    console.log('Version:', this.VERSION);
    console.log('Key prefix:', this.KEY_PREFIX);
    console.log('Statistics:', stats);
    console.log('Index:', JSON.parse(asyncStorage.getItem(this.INDEX_KEY) || '{}'));
    console.log('AsyncStorage pending writes:', asyncStorage.getStats());
    console.groupEnd();
  }
}