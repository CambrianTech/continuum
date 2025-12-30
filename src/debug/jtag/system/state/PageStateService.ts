/**
 * PageStateService - Single source of truth for current page/route state
 *
 * Part of the Scoped State Architecture (see docs/SCOPED-STATE-ARCHITECTURE.md):
 *   Site State → Page State → Widget State → Control State
 *
 * This service provides:
 * - Centralized page state that widgets subscribe to
 * - URL parsed ONCE by router, then state shared everywhere
 * - No timing issues - state set before widget created
 * - Clean override pattern - widget can read from state OR use local override
 *
 * Usage:
 *   // Router (MainWidget) sets state after URL parsing
 *   pageState.setContent('chat', 'general', { id: '...', uniqueId: 'general', displayName: 'General' });
 *
 *   // Widgets read state
 *   const state = pageState.getContent();
 *
 *   // Widgets subscribe to changes
 *   const unsubscribe = pageState.subscribe((state) => {
 *     if (state.contentType === 'chat') {
 *       this.switchToRoom(state.entityId);
 *     }
 *   });
 */

import type { UUID } from '../core/types/CrossPlatformUUID';

/**
 * Resolved entity information from RoutingService
 */
export interface ResolvedEntity {
  id: UUID;
  uniqueId: string;
  displayName: string;
}

/**
 * Current page state
 */
export interface PageState {
  /** Content type from URL path (chat, settings, persona, etc.) */
  contentType: string;
  /** Entity identifier from URL (uniqueId or UUID) */
  entityId?: string;
  /** Resolved entity with UUID and display info (after RoutingService lookup) */
  resolved?: ResolvedEntity;
  /** Timestamp when state was last updated */
  updatedAt: number;
}

/**
 * Callback type for page state subscribers
 */
export type PageStateListener = (state: PageState) => void;

/**
 * PageStateService implementation
 */
class PageStateServiceImpl {
  private state: PageState | null = null;
  private listeners: Set<PageStateListener> = new Set();

  /**
   * Set current page content (called by router after URL parsing)
   *
   * IMPORTANT: This should be called BEFORE creating widgets so they can
   * read the current state during initialization.
   */
  setContent(contentType: string, entityId?: string, resolved?: ResolvedEntity): void {
    this.state = {
      contentType,
      entityId,
      resolved,
      updatedAt: Date.now()
    };

    console.log(
      `📄 PageState: ${contentType}${entityId ? `/${entityId}` : ''}` +
      (resolved ? ` → ${resolved.displayName}` : '')
    );

    this.notifyListeners();
  }

  /**
   * Get current page state
   * Returns null if no state has been set (e.g., during initial load)
   */
  getContent(): PageState | null {
    return this.state;
  }

  /**
   * Get current content type (convenience method)
   */
  get contentType(): string | undefined {
    return this.state?.contentType;
  }

  /**
   * Get current entity ID (convenience method)
   */
  get entityId(): string | undefined {
    return this.state?.entityId;
  }

  /**
   * Get resolved entity (convenience method)
   */
  get resolved(): ResolvedEntity | undefined {
    return this.state?.resolved;
  }

  /**
   * Check if current page is a specific content type
   */
  isContentType(type: string): boolean {
    return this.state?.contentType === type;
  }

  /**
   * Subscribe to page state changes
   *
   * Immediately calls callback with current state if it exists,
   * then calls callback on every future state change.
   *
   * @returns Unsubscribe function - call this in disconnectedCallback()
   */
  subscribe(callback: PageStateListener): () => void {
    this.listeners.add(callback);

    // Immediately notify with current state if exists
    if (this.state) {
      try {
        callback(this.state);
      } catch (error) {
        console.error('📄 PageState: Error in subscriber callback:', error);
      }
    }

    // Return unsubscribe function
    return () => {
      this.listeners.delete(callback);
    };
  }

  /**
   * Clear state (useful for testing or logout)
   */
  clear(): void {
    this.state = null;
  }

  /**
   * Get subscriber count (for debugging)
   */
  get subscriberCount(): number {
    return this.listeners.size;
  }

  /**
   * Notify all listeners of state change
   */
  private notifyListeners(): void {
    if (!this.state) return;

    for (const listener of this.listeners) {
      try {
        listener(this.state);
      } catch (error) {
        console.error('📄 PageState: Error in subscriber callback:', error);
      }
    }
  }
}

/**
 * Singleton instance
 */
export const pageState = new PageStateServiceImpl();

/**
 * Export class for type purposes (do not instantiate directly)
 */
export type { PageStateServiceImpl as PageStateService };
