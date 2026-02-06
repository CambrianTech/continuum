/**
 * IndexedDBBackend - Async storage that doesn't block main thread
 *
 * Replaces LocalStorageDataBackend to prevent main thread blocking.
 * IndexedDB operations are async by design - no blocking.
 *
 * Collections to cache are defined in OFFLINE_CACHEABLE_COLLECTIONS (Constants.ts)
 * This file just reads from that single source of truth.
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';
import { OFFLINE_CACHEABLE_COLLECTIONS } from '../../../system/shared/Constants';

const DB_NAME = 'jtag-offline-cache';
const DB_VERSION = 3; // Bumped: using OFFLINE_CACHEABLE_COLLECTIONS from Constants.ts

let dbInstance: IDBDatabase | null = null;
let dbInitPromise: Promise<IDBDatabase> | null = null;

// Internal-only collection for sync queue (not exposed as cacheable entity)
const INTERNAL_COLLECTIONS = ['sync_queue'] as const;

// All collections = cacheable entities + internal collections
const ALL_COLLECTIONS = [...OFFLINE_CACHEABLE_COLLECTIONS, ...INTERNAL_COLLECTIONS];

/**
 * Get or initialize the IndexedDB database
 */
async function getDB(): Promise<IDBDatabase> {
  if (dbInstance) return dbInstance;

  if (dbInitPromise) return dbInitPromise;

  dbInitPromise = new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);

    request.onerror = () => {
      console.error('IndexedDB open error:', request.error);
      reject(request.error);
    };

    request.onsuccess = () => {
      dbInstance = request.result;
      resolve(dbInstance);
    };

    request.onupgradeneeded = (event) => {
      const db = (event.target as IDBOpenDBRequest).result;

      // Create object stores for all collections (cacheable entities + internal)
      for (const collection of ALL_COLLECTIONS) {
        if (!db.objectStoreNames.contains(collection)) {
          db.createObjectStore(collection, { keyPath: 'id' });
        }
      }
    };
  });

  return dbInitPromise;
}

/**
 * Check if a collection exists in IndexedDB
 */
async function hasCollection(collection: string): Promise<boolean> {
  try {
    const db = await getDB();
    return db.objectStoreNames.contains(collection);
  } catch {
    return false;
  }
}

/**
 * IndexedDBBackend - Static methods for async CRUD operations
 */
export class IndexedDBBackend {
  /**
   * Read entity from IndexedDB
   */
  static async read<T extends BaseEntity>(
    collection: string,
    id: UUID
  ): Promise<{ success: boolean; data?: T; error?: string }> {
    try {
      const db = await getDB();
      return new Promise((resolve) => {
        const tx = db.transaction(collection, 'readonly');
        const store = tx.objectStore(collection);
        const request = store.get(id);

        request.onsuccess = () => {
          if (request.result) {
            resolve({ success: true, data: request.result as T });
          } else {
            resolve({ success: false, error: 'Not found' });
          }
        };

        request.onerror = () => {
          resolve({ success: false, error: request.error?.message || 'Read failed' });
        };
      });
    } catch (err) {
      return { success: false, error: (err as Error).message };
    }
  }

  /**
   * Create entity in IndexedDB
   */
  static async create<T extends BaseEntity>(
    collection: string,
    data: T
  ): Promise<{ success: boolean; error?: string }> {
    try {
      const db = await getDB();
      return new Promise((resolve) => {
        const tx = db.transaction(collection, 'readwrite');
        const store = tx.objectStore(collection);
        const request = store.add(data);

        request.onsuccess = () => {
          resolve({ success: true });
        };

        request.onerror = () => {
          resolve({ success: false, error: request.error?.message || 'Create failed' });
        };
      });
    } catch (err) {
      return { success: false, error: (err as Error).message };
    }
  }

  /**
   * Update entity in IndexedDB (put = upsert)
   */
  static async update<T extends BaseEntity>(
    collection: string,
    id: UUID,
    data: Partial<T>
  ): Promise<{ success: boolean; error?: string }> {
    try {
      const db = await getDB();
      return new Promise((resolve) => {
        const tx = db.transaction(collection, 'readwrite');
        const store = tx.objectStore(collection);

        // First get existing, then merge
        const getRequest = store.get(id);

        getRequest.onsuccess = () => {
          const existing = getRequest.result || { id };
          const merged = { ...existing, ...data, id };

          const putRequest = store.put(merged);

          putRequest.onsuccess = () => {
            resolve({ success: true });
          };

          putRequest.onerror = () => {
            resolve({ success: false, error: putRequest.error?.message || 'Update failed' });
          };
        };

        getRequest.onerror = () => {
          resolve({ success: false, error: getRequest.error?.message || 'Read for update failed' });
        };
      });
    } catch (err) {
      return { success: false, error: (err as Error).message };
    }
  }

  /**
   * Delete entity from IndexedDB
   */
  static async delete(
    collection: string,
    id: UUID
  ): Promise<{ success: boolean; error?: string }> {
    try {
      const db = await getDB();
      return new Promise((resolve) => {
        const tx = db.transaction(collection, 'readwrite');
        const store = tx.objectStore(collection);
        const request = store.delete(id);

        request.onsuccess = () => {
          resolve({ success: true });
        };

        request.onerror = () => {
          resolve({ success: false, error: request.error?.message || 'Delete failed' });
        };
      });
    } catch (err) {
      return { success: false, error: (err as Error).message };
    }
  }
}
