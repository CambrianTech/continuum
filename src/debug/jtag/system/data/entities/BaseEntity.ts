/**
 * Base Entity - All entities extend this
 */

import { PrimaryField, DateField, NumberField } from '../decorators/FieldDecorators';
import { generateUUID, UUID } from '../../core/types/CrossPlatformUUID';
import { getDataEventName } from '../../../commands/data/shared/DataEventConstants';

export abstract class BaseEntity {
  [key: string]: unknown;

  @PrimaryField()
  id: UUID; //TODO; all entities must have id field

  @DateField({ index: true })
  createdAt: Date;

  @DateField({ index: true })
  updatedAt: Date;

  @NumberField()
  version: number;

  constructor() {
    this.id = generateUUID();
    this.createdAt = new Date();
    this.updatedAt = new Date();
    this.version = 0; //0 indeicates new record
  }

  /**
   * Protected utility methods for entity validation
   */

  /**
   * Serde-like date validation - accepts Date objects or ISO strings
   * Protected method available to all entities for consistent date validation
   */
  protected isValidDate(value: any): boolean {
    if (!value) return false;

    // Accept Date objects
    if (value instanceof Date) {
      return !isNaN(value.getTime());
    }

    // Accept ISO date strings
    if (typeof value === 'string') {
      const dateObj = new Date(value);
      return !isNaN(dateObj.getTime());
    }

    return false;
  }

  /**
   * Abstract methods that child entities must implement
   */
  abstract get collection(): string;
  abstract validate(): { success: boolean; error?: string };

  /**
   * Factory method to create entities with validation
   */
  static create<T extends BaseEntity>(
    this: new() => T,
    data: Partial<T>
  ): { success: boolean; entity?: T; error?: string } {
    try {
      const entity = new this();

      // Apply provided data
      Object.assign(entity, data);

      // Validate the entity
      const validation = entity.validate();
      if (!validation.success) {
        return { success: false, error: validation.error };
      }

      return { success: true, entity };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Entity creation failed'
      };
    }
  }

  /**
   * Factory method to create entities for testing with string IDs
   */
  static createForTest<T extends BaseEntity>(
    this: typeof BaseEntity,
    testId: string,
    data: Partial<T>
  ): { success: boolean; entity?: T; error?: string } {
    const result = (this as any).create(data);
    if (result.success && result.entity) {
      // Convert string test ID to UUID format for testing
      result.entity.id = testId as UUID;
    }
    return result;
  }

  /**
   * Get the collection name from the entity instance
   */
  getCollectionName(): string {
    return this.collection;
  }

  /**
   * Get schema information for this entity type
   */
  static getSchema<T extends BaseEntity>(this: new() => T): Record<string, unknown> {
    const instance = new this();
    return {
      collection: instance.collection,
      fields: {
        id: { type: 'UUID', required: true, primary: true },
        createdAt: { type: 'Date', required: true, index: true },
        updatedAt: { type: 'Date', required: true, index: true },
        version: { type: 'number', required: true }
      }
    };
  }

  /**
   * Validate data against schema for this entity type
   */
  static validateData<T extends BaseEntity>(
    this: typeof BaseEntity,
    data: Partial<T>
  ): { success: boolean; error?: string; validatedData?: T } {
    const result = (this as any).create(data);
    return {
      success: result.success,
      error: result.error,
      validatedData: result.entity
    };
  }

  /**
   * Create event name for this entity and action
   */
  static getEventName(collection: string, action: 'created' | 'updated' | 'deleted'): string {
    return getDataEventName(collection, action);
  }

  /**
   * Create entity event data for real-time updates
   */
  static createEntityEvent<T extends BaseEntity>(entity: T, eventType: string): { data: T; type: string } {
    return {
      data: entity,
      type: eventType
    };
  }
}