/**
 * Base Entity - All entities extend this
 */

import { PrimaryField, DateField, NumberField } from '../decorators/FieldDecorators';
import { generateUUID } from '../../core/types/CrossPlatformUUID';

export abstract class BaseEntity {
  [key: string]: unknown;

  @PrimaryField()
  id: string;

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
    this.version = 1;
  }

  /**
   * Static factory method for creating events from entity data
   */
  static createEntityEvent<E extends BaseEntity, T extends Record<string, any>>(
    entityData: E,
    eventType: string,
    additionalData?: T
  ) {
    return {
      eventType,
      timestamp: new Date().toISOString(),
      entity: entityData,
      ...additionalData
    };
  }
}