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