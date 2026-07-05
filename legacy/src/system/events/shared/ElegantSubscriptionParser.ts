/**
 * Elegant Event Subscription Parser
 *
 * Parses patterns like:
 * - 'data:users' -> All user CRUD operations
 * - 'data:users {created,updated}' -> Only creates/updates
 * - 'data:users {c,u}' -> Shorthand
 * - 'data:users !{deleted}' -> Exclude deletes
 */

export type CRUDAction = 'created' | 'updated' | 'deleted';

export interface SubscriptionPattern {
  collection: string;
  actions: CRUDAction[];
  entityId?: string;
  exclude: boolean;
}

export interface SubscriptionFilter {
  where?: Record<string, any>;
}

export interface ParsedSubscription {
  pattern: SubscriptionPattern;
  filter?: SubscriptionFilter;
}

/**
 * Parse elegant subscription patterns
 */
export class ElegantSubscriptionParser {

  private static ACTION_SHORTCUTS: Record<string, CRUDAction> = {
    'c': 'created',
    'u': 'updated',
    'd': 'deleted'
  };

  /**
   * Parse subscription pattern string into structured data
   *
   * @example
   * parsePattern('data:users {created,updated}')
   * // → { collection: 'users', actions: ['created', 'updated'], exclude: false }
   *
   * parsePattern('data:users:550e8400-e29b-41d4-a716-446655440000 {updated}')
   * // → { collection: 'users', actions: ['updated'], entityId: '550e8400-e29b-41d4-a716-446655440000', exclude: false }
   */
  static parsePattern(patternString: string): SubscriptionPattern {
    // Remove whitespace
    const cleaned = patternString.trim();

    // Match pattern: data:collection[:entityId] [!]{action1,action2}
    const match = cleaned.match(/^data:(\w+)(?::([0-9a-f-]+))?(?:\s*(!?\{[^}]+\}))?$/i);

    if (!match) {
      throw new Error(`Invalid subscription pattern: ${patternString}`);
    }

    const [, collection, entityId, actionPart] = match;

    // Parse actions
    let actions: CRUDAction[] = ['created', 'updated', 'deleted']; // Default: all actions
    let exclude = false;

    if (actionPart) {
      exclude = actionPart.startsWith('!');
      const actionsStr = actionPart.replace(/^!?\{|\}$/g, '');
      const actionsList = actionsStr.split(',').map(a => a.trim());

      // Convert shortcuts and validate actions
      const parsedActions: CRUDAction[] = [];
      for (const action of actionsList) {
        const expandedAction = this.ACTION_SHORTCUTS[action] || action as CRUDAction;

        if (!['created', 'updated', 'deleted'].includes(expandedAction)) {
          throw new Error(`Invalid action: ${action}`);
        }

        parsedActions.push(expandedAction);
      }

      if (exclude) {
        // Exclude specified actions - include the others
        actions = (['created', 'updated', 'deleted'] as CRUDAction[])
          .filter(action => !parsedActions.includes(action));
      } else {
        // Include only specified actions
        actions = parsedActions;
      }
    }

    return {
      collection,
      actions,
      entityId,
      exclude: !!exclude
    };
  }

  /**
   * Check if an emitted event matches a subscription pattern
   *
   * @param eventName - Emitted event name like 'data:users:created'
   * @param pattern - Parsed subscription pattern
   * @returns true if event matches pattern
   */
  static matchesPattern(eventName: string, pattern: SubscriptionPattern): boolean {
    // Parse event name: data:collection:action or data:collection:entityId:action
    const eventMatch = eventName.match(/^data:(\w+):(?:([0-9a-f-]+):)?(\w+)$/i);

    if (!eventMatch) {
      return false;
    }

    const [, eventCollection, eventEntityId, eventAction] = eventMatch;

    // Check collection match
    if (eventCollection !== pattern.collection) {
      return false;
    }

    // Check entity ID match (if pattern specifies one)
    if (pattern.entityId && pattern.entityId !== eventEntityId) {
      return false;
    }

    // Check action match
    if (!pattern.actions.includes(eventAction as CRUDAction)) {
      return false;
    }

    return true;
  }

  /**
   * Check if event data matches subscription filters
   *
   * @param eventData - The entity data from the event
   * @param filter - The subscription filter
   * @returns true if data matches filters
   */
  static matchesFilter(eventData: any, filter?: SubscriptionFilter): boolean {
    if (!filter?.where) {
      return true; // No filters = match all
    }

    // Simple property matching - could be enhanced with complex queries
    for (const [key, value] of Object.entries(filter.where)) {
      if (eventData[key] !== value) {
        return false;
      }
    }

    return true;
  }

  /**
   * Extract action from event name
   *
   * @param eventName - Event name like 'data:users:created'
   * @returns The action ('created', 'updated', 'deleted') or null
   */
  static extractAction(eventName: string): CRUDAction | null {
    const match = eventName.match(/^data:\w+:(?:[0-9a-f-]+:)?(\w+)$/i);
    return match ? match[1] as CRUDAction : null;
  }

  /**
   * Create enhanced event data with action metadata
   *
   * @param eventName - Original event name
   * @param eventData - Original event data
   * @returns Enhanced event with action metadata
   */
  static createEnhancedEvent<T>(eventName: string, eventData: T) {
    const action = this.extractAction(eventName);

    return {
      action,
      data: eventData,
      originalEventName: eventName,
      timestamp: new Date().toISOString()
    };
  }
}