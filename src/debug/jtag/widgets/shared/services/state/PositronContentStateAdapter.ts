/**
 * PositronContentStateAdapter - Positron Pattern for Content State Management
 *
 * Implements the Positron principle: State drives UI, not DB refetch
 *
 * This adapter subscribes to content events (opened, closed, switched) and updates
 * local state directly from event data instead of refetching from the database.
 * This eliminates race conditions between event emission and DB persistence.
 *
 * Usage:
 *   const adapter = new PositronContentStateAdapter(
 *     () => this.userState,
 *     () => this.updateContentTabs()
 *   );
 *   adapter.subscribeToEvents();
 */

import { Events } from '@system/core/shared/Events';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { UserStateEntity, ContentItem, ContentType, ContentPriority } from '@system/data/entities/UserStateEntity';
import type { ContentOpenedEvent } from '@commands/collaboration/content/open/shared/ContentOpenTypes';

/**
 * Event data structures (match server-emitted events)
 */
interface ContentClosedEventData {
  contentItemId?: UUID;
  currentItemId?: UUID;
}

interface ContentSwitchedEventData {
  contentItemId?: UUID;
  currentItemId?: UUID;
  contentType?: string;
  entityId?: string;
  title?: string;
}

/**
 * Callback type for state change notifications
 */
type StateChangeCallback = () => void;

/**
 * Callback type for view switching
 */
type ViewSwitchCallback = (contentType: string, entityId?: string) => void;

/**
 * Callback type for URL updates
 */
type UrlUpdateCallback = (contentType: string, entityId?: string) => void;

/**
 * Configuration options for the adapter
 */
export interface PositronContentStateAdapterConfig {
  /** Optional name for debugging */
  name?: string;
  /** Called when state changes and UI should re-render */
  onStateChange: StateChangeCallback;
  /** Called when view should switch to different content */
  onViewSwitch?: ViewSwitchCallback;
  /** Called when URL should update */
  onUrlUpdate?: UrlUpdateCallback;
  /** Fallback function if userState not available */
  onFallback?: () => void;
}

/**
 * PositronContentStateAdapter
 *
 * Handles content state updates following Positron principles:
 * 1. Subscribe to events
 * 2. Update local state from event data (not DB)
 * 3. Trigger UI re-render from local state
 *
 * This pattern eliminates the race condition between:
 * - Command persisting to DB
 * - Event being emitted
 * - UI fetching from DB (might get stale data)
 */
export class PositronContentStateAdapter {
  private name: string;
  private getUserState: () => UserStateEntity | undefined;
  private config: PositronContentStateAdapterConfig;
  private subscribed = false;

  constructor(
    getUserState: () => UserStateEntity | undefined,
    config: PositronContentStateAdapterConfig
  ) {
    this.getUserState = getUserState;
    this.config = config;
    this.name = config.name || 'PositronContentStateAdapter';
  }

  /**
   * Subscribe to all content events
   * Call this during widget initialization
   */
  subscribeToEvents(): void {
    if (this.subscribed) {
      console.warn(`${this.name}: Already subscribed to events`);
      return;
    }

    Events.subscribe('content:opened', this.handleContentOpened.bind(this));
    Events.subscribe('content:closed', this.handleContentClosed.bind(this));
    Events.subscribe('content:switched', this.handleContentSwitched.bind(this));

    this.subscribed = true;
    console.log(`${this.name}: Subscribed to content events (Positron pattern)`);
  }

  /**
   * Handle content:opened event
   * Adds new content item to local state without DB refetch
   */
  private handleContentOpened(eventData: unknown): void {
    console.log(`${this.name}: Received content:opened event`, eventData);

    const data = eventData as ContentOpenedEvent & { setAsCurrent?: boolean };
    const userState = this.getUserState();

    // Fallback if state not available
    if (!userState?.contentState) {
      console.warn(`${this.name}: userState.contentState not initialized, using fallback`);
      this.config.onFallback?.();
      return;
    }

    // Check if content already exists in openItems
    const existingItem = userState.contentState.openItems.find(
      item => item.id === data.contentItemId ||
              (item.type === data.contentType && item.entityId === data.entityId)
    );

    if (!existingItem && data.contentItemId) {
      // Add new content item to local state (Positron: state drives UI)
      const newItem: ContentItem = {
        id: data.contentItemId,
        type: data.contentType,
        entityId: data.entityId,
        title: data.title || data.contentType,
        lastAccessedAt: new Date(),
        priority: 'normal' as ContentPriority
      };
      userState.contentState.openItems.push(newItem);
      console.log(`${this.name}: Added new tab to local state: ${newItem.title}`);
    } else if (existingItem) {
      // Update lastAccessedAt for existing item
      existingItem.lastAccessedAt = new Date();
      console.log(`${this.name}: Updated existing tab: ${existingItem.title}`);
    }

    // Set as current if requested
    if (data.setAsCurrent && data.contentItemId) {
      userState.contentState.currentItemId = data.contentItemId;
    }

    // Notify UI to re-render from local state (no DB fetch!)
    this.config.onStateChange();

    // Switch view if requested
    if (data.setAsCurrent && data.contentType) {
      console.log(`${this.name}: Switching to new ${data.contentType} content`);
      this.config.onViewSwitch?.(data.contentType, data.entityId);
      this.config.onUrlUpdate?.(data.contentType, data.entityId);
    }
  }

  /**
   * Handle content:closed event
   * Removes content item from local state without DB refetch
   */
  private handleContentClosed(eventData: unknown): void {
    console.log(`${this.name}: Received content:closed event`, eventData);

    const data = eventData as ContentClosedEventData;
    const userState = this.getUserState();

    // Fallback if state not available
    if (!userState?.contentState) {
      console.warn(`${this.name}: userState.contentState not initialized, using fallback`);
      this.config.onFallback?.();
      return;
    }

    if (data.contentItemId) {
      // Remove closed item from local state (Positron: state drives UI)
      userState.contentState.openItems = userState.contentState.openItems.filter(
        item => item.id !== data.contentItemId
      );
      console.log(`${this.name}: Removed closed tab from local state`);

      // Update currentItemId if provided
      if (data.currentItemId) {
        userState.contentState.currentItemId = data.currentItemId;
      } else if (userState.contentState.openItems.length > 0) {
        // Fallback to most recent item if current was closed
        userState.contentState.currentItemId = userState.contentState.openItems[
          userState.contentState.openItems.length - 1
        ].id;
      } else {
        userState.contentState.currentItemId = undefined;
      }
    }

    // Notify UI to re-render from local state (no DB fetch!)
    this.config.onStateChange();
  }

  /**
   * Handle content:switched event
   * Updates current item in local state without DB refetch
   */
  private handleContentSwitched(eventData: unknown): void {
    console.log(`${this.name}: Received content:switched event`, eventData);

    const data = eventData as ContentSwitchedEventData;
    const userState = this.getUserState();

    // Fallback if state not available
    if (!userState?.contentState) {
      console.warn(`${this.name}: userState.contentState not initialized, using fallback`);
      this.config.onFallback?.();
      return;
    }

    // Update currentItemId in local state (Positron: state drives UI)
    if (data.currentItemId) {
      userState.contentState.currentItemId = data.currentItemId;
      console.log(`${this.name}: Set current tab in local state: ${data.currentItemId}`);
    }

    // Update lastAccessedAt for the switched-to item
    const switchedItem = userState.contentState.openItems.find(
      item => item.id === data.contentItemId
    );
    if (switchedItem) {
      switchedItem.lastAccessedAt = new Date();
    }

    // Notify UI to re-render from local state (no DB fetch!)
    this.config.onStateChange();

    // Switch view
    if (data.contentType) {
      console.log(`${this.name}: Switching view to ${data.contentType}`);
      this.config.onViewSwitch?.(data.contentType, data.entityId);
      this.config.onUrlUpdate?.(data.contentType, data.entityId);
    }
  }

  /**
   * Manually add a content item to local state
   * Useful for optimistic UI when opening content directly
   */
  addContentItem(item: Omit<ContentItem, 'lastAccessedAt'>): void {
    const userState = this.getUserState();
    if (!userState?.contentState) {
      console.warn(`${this.name}: Cannot add item - userState.contentState not available`);
      return;
    }

    // Check if already exists
    const existing = userState.contentState.openItems.find(
      i => i.id === item.id || (i.type === item.type && i.entityId === item.entityId)
    );

    if (!existing) {
      const newItem: ContentItem = {
        ...item,
        lastAccessedAt: new Date()
      };
      userState.contentState.openItems.push(newItem);
      this.config.onStateChange();
    }
  }

  /**
   * Set the current content item
   */
  setCurrentItem(itemId: UUID): void {
    const userState = this.getUserState();
    if (!userState?.contentState) {
      console.warn(`${this.name}: Cannot set current - userState.contentState not available`);
      return;
    }

    userState.contentState.currentItemId = itemId;
    this.config.onStateChange();
  }
}
