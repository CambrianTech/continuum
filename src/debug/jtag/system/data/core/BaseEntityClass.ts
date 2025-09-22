/**
 * BaseEntity Class - Generic Event Factory for Entities
 *
 * Provides type-safe event creation for any entity that extends BaseEntity interface
 * Ensures entity field consistency in events by letting the entity control event creation
 */

import type { BaseEntity } from '../domains/CoreTypes';
import type { ISOString } from '../domains/CoreTypes';

/**
 * Abstract base class that provides generic event factory method
 * Any entity class can extend this to get consistent event creation
 */
export abstract class BaseEntityClass implements BaseEntity {
  // BaseEntity interface requirements
  id: string;
  createdAt: ISOString;
  updatedAt: ISOString;
  version: number;

  constructor() {
    this.id = '';
    this.createdAt = new Date().toISOString() as ISOString;
    this.updatedAt = new Date().toISOString() as ISOString;
    this.version = 1;
  }

  /**
   * Generic event factory method - ensures entity controls its own event structure
   * @param eventType - The event type constant (e.g., CHAT_EVENTS.MESSAGE_RECEIVED)
   * @param additionalData - Any additional event-specific data
   * @returns Type-safe event data with the entity as the primary payload
   */
  createEvent<T extends Record<string, any> = {}>(eventType: string, additionalData?: T): any {
    return {
      eventType,
      timestamp: new Date().toISOString(),
      entity: this, // The entity itself is the source of truth
      ...additionalData
    };
  }

  /**
   * Static factory method for creating events from plain data objects
   * Useful when you have data that conforms to an entity but isn't a class instance
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