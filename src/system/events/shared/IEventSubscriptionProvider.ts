/**
 * Interface for daemons that provide event subscription capabilities
 * Used by JTAGClient to access unified event subscriptions in local connections
 */

import type { EventSubscriptionManager } from './EventSubscriptionManager';

export interface IEventSubscriptionProvider {
  /**
   * Get the subscription manager for this daemon
   * Allows clients to subscribe to events using unified patterns
   */
  getSubscriptionManager(): EventSubscriptionManager;
}

/**
 * Type guard to check if a daemon implements IEventSubscriptionProvider
 */
export function isEventSubscriptionProvider(daemon: unknown): daemon is IEventSubscriptionProvider {
  return (
    daemon !== null &&
    daemon !== undefined &&
    typeof daemon === 'object' &&
    'getSubscriptionManager' in daemon &&
    typeof (daemon as Record<string, unknown>).getSubscriptionManager === 'function'
  );
}