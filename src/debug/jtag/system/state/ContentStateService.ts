/**
 * ContentStateService - Single source of truth for content/tab state
 *
 * React-style architecture:
 * - State lives in ONE place (this singleton)
 * - Widgets SUBSCRIBE to changes
 * - Updates trigger all subscribers
 * - No direct widget-to-widget calls
 *
 * This replaces the broken pattern where each widget had its own
 * userState.contentState copy that diverged on optimistic updates.
 *
 * Usage:
 *   // Initialize once (MainWidget on startup)
 *   contentState.initialize(openItems, currentItemId);
 *
 *   // Update state (any widget)
 *   contentState.addItem(newItem);
 *   contentState.removeItem(itemId);
 *   contentState.setCurrent(itemId);
 *
 *   // Subscribe to changes (ContentTabsWidget, etc.)
 *   contentState.subscribe((state) => this.renderTabs(state.openItems));
 */

import type { UUID } from '../core/types/CrossPlatformUUID';
import type { ContentItem, ContentType, ContentPriority } from '../data/entities/UserStateEntity';

/**
 * Content state structure
 */
export interface ContentStateData {
  openItems: ContentItem[];
  currentItemId?: UUID;
}

/**
 * Callback type for content state subscribers
 */
export type ContentStateListener = (state: ContentStateData) => void;

/**
 * ContentStateService implementation
 */
class ContentStateServiceImpl {
  private state: ContentStateData = {
    openItems: [],
    currentItemId: undefined
  };
  private listeners: Set<ContentStateListener> = new Set();
  private initialized = false;
  private _notifyPending = false;

  /**
   * Initialize from persisted userState (call once on app load)
   * GUARDED: Skips if already initialized with same data
   */
  initialize(openItems: ContentItem[], currentItemId?: UUID): void {
    // Guard: Skip if already initialized with same item count
    // (prevents redundant re-initialization from multiple sources)
    if (this.initialized && this.state.openItems.length === openItems.length) {
      return; // Already initialized, skip
    }

    this.state = {
      openItems: [...openItems],
      currentItemId
    };
    this.initialized = true;
    console.log(`📋 ContentState: Initialized with ${openItems.length} items`);
    this.scheduleNotify();
  }

  /**
   * Check if initialized
   */
  get isInitialized(): boolean {
    return this.initialized;
  }

  /**
   * Get current state (read-only snapshot)
   */
  getState(): Readonly<ContentStateData> {
    return this.state;
  }

  /**
   * Get open items
   */
  get openItems(): readonly ContentItem[] {
    return this.state.openItems;
  }

  /**
   * Get current item ID
   */
  get currentItemId(): UUID | undefined {
    return this.state.currentItemId;
  }

  /**
   * Get current item
   */
  get currentItem(): ContentItem | undefined {
    return this.state.openItems.find(item => item.id === this.state.currentItemId);
  }

  /**
   * Add a new content item (opens a tab)
   * Returns the item (with ID if generated)
   */
  addItem(item: Omit<ContentItem, 'id' | 'lastAccessedAt'> & { id?: UUID }, setAsCurrent = true): ContentItem {
    // Generate temp ID and timestamp if not provided
    const newItem: ContentItem = {
      ...item,
      id: item.id || (`temp-${Date.now()}` as UUID),
      lastAccessedAt: new Date()
    };

    // Check if already exists by type+entityId
    const existing = this.state.openItems.find(
      i => i.type === newItem.type && i.entityId === newItem.entityId
    );

    if (existing) {
      // Update existing and set as current
      existing.lastAccessedAt = new Date();
      if (setAsCurrent) {
        this.state.currentItemId = existing.id;
      }
      console.log(`📋 ContentState: Updated existing ${existing.type}/${existing.entityId || 'default'}`);
    } else {
      // Add new item
      this.state.openItems.push(newItem);
      if (setAsCurrent) {
        this.state.currentItemId = newItem.id;
      }
      console.log(`📋 ContentState: Added ${newItem.type}/${newItem.entityId || 'default'}`);
    }

    this.scheduleNotify();
    return existing || newItem;
  }

  /**
   * Remove a content item (closes a tab)
   * Switches to last remaining item if closing current
   */
  removeItem(itemId: UUID): void {
    const wasCurrentItem = this.state.currentItemId === itemId;

    // Remove item
    this.state.openItems = this.state.openItems.filter(item => item.id !== itemId);

    // If we closed the current item, switch to last remaining
    if (wasCurrentItem && this.state.openItems.length > 0) {
      const lastItem = this.state.openItems[this.state.openItems.length - 1];
      this.state.currentItemId = lastItem.id;
    } else if (this.state.openItems.length === 0) {
      this.state.currentItemId = undefined;
    }

    console.log(`📋 ContentState: Removed item, ${this.state.openItems.length} remaining`);
    this.scheduleNotify();
  }

  /**
   * Set current item (switch tabs)
   */
  setCurrent(itemId: UUID): void {
    const item = this.state.openItems.find(i => i.id === itemId);
    if (item) {
      this.state.currentItemId = itemId;
      item.lastAccessedAt = new Date();
      console.log(`📋 ContentState: Switched to ${item.type}/${item.entityId || 'default'}`);
      this.scheduleNotify();
    } else {
      console.warn(`📋 ContentState: Item ${itemId} not found`);
    }
  }

  /**
   * Find item by type and entityId
   */
  findItem(type: string, entityId?: string): ContentItem | undefined {
    return this.state.openItems.find(
      item => item.type === type && item.entityId === entityId
    );
  }

  /**
   * Update item's real ID after server confirms
   * (replaces temp ID with server-generated ID)
   */
  updateItemId(tempId: UUID, realId: UUID): void {
    const item = this.state.openItems.find(i => i.id === tempId);
    if (item) {
      item.id = realId;
      if (this.state.currentItemId === tempId) {
        this.state.currentItemId = realId;
      }
      console.log(`📋 ContentState: Updated temp ID ${tempId} → ${realId}`);
      this.scheduleNotify();
    }
  }

  /**
   * Subscribe to state changes
   * Immediately calls callback with current state, then on every change
   * Returns unsubscribe function
   */
  subscribe(callback: ContentStateListener): () => void {
    this.listeners.add(callback);

    // Immediately notify with current state
    try {
      callback(this.state);
    } catch (error) {
      console.error('📋 ContentState: Error in subscriber callback:', error);
    }

    // Return unsubscribe function
    return () => {
      this.listeners.delete(callback);
    };
  }

  /**
   * Get subscriber count (for debugging)
   */
  get subscriberCount(): number {
    return this.listeners.size;
  }

  /**
   * Schedule async notification to all listeners.
   * Uses queueMicrotask to batch rapid state changes into single notification.
   */
  private scheduleNotify(): void {
    if (this._notifyPending) return;
    this._notifyPending = true;
    queueMicrotask(() => {
      this._notifyPending = false;
      this.flushNotifications();
    });
  }

  /**
   * Actually notify all listeners (called from microtask)
   */
  private flushNotifications(): void {
    for (const listener of this.listeners) {
      try {
        listener(this.state);
      } catch (error) {
        console.error('📋 ContentState: Error in subscriber callback:', error);
      }
    }
  }

  /**
   * Reset state (for testing or logout)
   */
  reset(): void {
    this.state = { openItems: [], currentItemId: undefined };
    this.initialized = false;
    this.scheduleNotify();
  }
}

/**
 * Singleton instance - THE source of truth for content state
 */
export const contentState = new ContentStateServiceImpl();
