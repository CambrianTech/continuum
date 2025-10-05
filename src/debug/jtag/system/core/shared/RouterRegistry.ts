/**
 * Router Registry - Global router discovery
 *
 * Enables Events.emit() to automatically find the router for any context
 * without requiring manual passing of commander/router objects
 */

import type { JTAGContext } from '../types/JTAGTypes';
import type { JTAGRouter } from '../router/shared/JTAGRouter';

export class RouterRegistry {
  private static routers = new Map<string, JTAGRouter>();

  /**
   * Register a router for a context
   * Called by daemons during initialization
   */
  static register(context: JTAGContext, router: JTAGRouter): void {
    const key = this.getContextKey(context);
    this.routers.set(key, router);
    console.log(`🗂️ RouterRegistry: Registered router for context ${context.environment}/${context.uuid}`);
  }

  /**
   * Get router for a context
   * Used by Events.emit() to automatically discover routing
   */
  static getForContext(context: JTAGContext): JTAGRouter | undefined {
    const key = this.getContextKey(context);
    const router = this.routers.get(key);

    if (!router) {
      console.warn(`⚠️ RouterRegistry: No router found for context ${context.environment}/${context.uuid}`);
    }

    return router;
  }

  /**
   * Unregister a router (cleanup on shutdown)
   */
  static unregister(context: JTAGContext): void {
    const key = this.getContextKey(context);
    this.routers.delete(key);
    console.log(`🗂️ RouterRegistry: Unregistered router for context ${context.environment}/${context.uuid}`);
  }

  /**
   * Get unique key for context
   */
  private static getContextKey(context: JTAGContext): string {
    return `${context.environment}:${context.uuid}`;
  }

  /**
   * Check if router exists for context
   */
  static has(context: JTAGContext): boolean {
    const key = this.getContextKey(context);
    return this.routers.has(key);
  }
}
