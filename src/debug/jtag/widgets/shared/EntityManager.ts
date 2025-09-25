/**
 * EntityManager - Centralized entity state management with proper debugging
 *
 * Provides a clean class-based interface for managing collections of entities
 * with built-in deduplication, state inspection, and clear mutation control.
 */

import type { BaseEntity } from '../../system/data/entities/BaseEntity';

export interface EntityManagerConfig {
  readonly name: string; // For debugging purposes
  readonly maxSize?: number; // Optional size limit
  readonly debugMode?: boolean; // Extra logging
}

export class EntityManager<T extends BaseEntity> {
  private entities: T[] = [];
  private readonly config: EntityManagerConfig;
  private readonly entityIdMap = new Map<string, number>(); // ID -> index mapping for fast lookup

  constructor(config: EntityManagerConfig) {
    this.config = config;
    if (config.debugMode) {
      console.log(`🏗️ EntityManager: Created "${config.name}" with debug mode enabled`);
    }
  }

  /**
   * Get entity ID with fallback for different ID field names
   */
  private getEntityId(entity: T): string {
    return entity.id || (entity as any).messageId || 'unknown';
  }

  /**
   * Rebuild the ID mapping from current entities array
   */
  private rebuildIndexMap(): void {
    this.entityIdMap.clear();
    this.entities.forEach((entity, index) => {
      const id = this.getEntityId(entity);
      this.entityIdMap.set(id, index);
    });
  }

  /**
   * Add entity with automatic deduplication
   * Returns true if added, false if duplicate was blocked
   */
  add(entity: T): boolean {
    const entityId = this.getEntityId(entity);

    // Check for duplicate using fast Map lookup
    if (this.entityIdMap.has(entityId)) {
      if (this.config.debugMode) {
        console.warn(`⚠️ EntityManager "${this.config.name}": Blocked duplicate entity ID: ${entityId}`);
      }
      return false;
    }

    // Check size limit
    if (this.config.maxSize && this.entities.length >= this.config.maxSize) {
      console.warn(`⚠️ EntityManager "${this.config.name}": Size limit reached (${this.config.maxSize}), rejecting add`);
      return false;
    }

    // Add entity and update index map
    const newIndex = this.entities.length;
    this.entities.push(entity);
    this.entityIdMap.set(entityId, newIndex);

    if (this.config.debugMode) {
      console.log(`✅ EntityManager "${this.config.name}": Added entity ID: ${entityId}, total: ${this.entities.length}`);
    }

    return true;
  }

  /**
   * Update existing entity by ID
   * Returns true if updated, false if not found
   */
  update(entityId: string, entity: T): boolean {
    const index = this.entityIdMap.get(entityId);
    if (index === undefined) {
      if (this.config.debugMode) {
        console.warn(`⚠️ EntityManager "${this.config.name}": Cannot update unknown entity ID: ${entityId}`);
      }
      return false;
    }

    this.entities[index] = entity;

    if (this.config.debugMode) {
      console.log(`🔄 EntityManager "${this.config.name}": Updated entity ID: ${entityId} at index ${index}`);
    }

    return true;
  }

  /**
   * Remove entity by ID
   * Returns true if removed, false if not found
   */
  remove(entityId: string): boolean {
    const index = this.entityIdMap.get(entityId);
    if (index === undefined) {
      return false;
    }

    this.entities.splice(index, 1);
    this.rebuildIndexMap(); // Rebuild since indices changed

    if (this.config.debugMode) {
      console.log(`🗑️ EntityManager "${this.config.name}": Removed entity ID: ${entityId}, total: ${this.entities.length}`);
    }

    return true;
  }

  /**
   * Get entity by ID
   */
  get(entityId: string): T | undefined {
    const index = this.entityIdMap.get(entityId);
    return index !== undefined ? this.entities[index] : undefined;
  }

  /**
   * Check if entity exists
   */
  has(entityId: string): boolean {
    return this.entityIdMap.has(entityId);
  }

  /**
   * Get all entities (read-only)
   */
  getAll(): readonly T[] {
    return [...this.entities];
  }

  /**
   * Get entity count
   */
  count(): number {
    return this.entities.length;
  }

  /**
   * Clear all entities
   */
  clear(): void {
    this.entities = [];
    this.entityIdMap.clear();

    if (this.config.debugMode) {
      console.log(`🧹 EntityManager "${this.config.name}": Cleared all entities`);
    }
  }

  /**
   * Get debug information
   */
  getDebugInfo(): object {
    return {
      name: this.config.name,
      entityCount: this.entities.length,
      mapSize: this.entityIdMap.size,
      maxSize: this.config.maxSize,
      entities: this.entities.map(e => ({
        id: this.getEntityId(e),
        type: e.constructor.name
      }))
    };
  }

  /**
   * Validate internal consistency (for debugging)
   */
  validate(): boolean {
    // Check that map size matches array length
    if (this.entityIdMap.size !== this.entities.length) {
      console.error(`❌ EntityManager "${this.config.name}": Map size (${this.entityIdMap.size}) doesn't match array length (${this.entities.length})`);
      return false;
    }

    // Check that all mapped indices are valid
    for (const [id, index] of this.entityIdMap) {
      if (index >= this.entities.length || this.getEntityId(this.entities[index]) !== id) {
        console.error(`❌ EntityManager "${this.config.name}": Invalid mapping for ID ${id} -> index ${index}`);
        return false;
      }
    }

    return true;
  }
}