/**
 * ContentService - Single source of truth for content/tab/URL management
 *
 * ALL content operations go through this service:
 * - Opening content (creates tab, switches view, updates URL, persists)
 * - Switching tabs (sets current, updates URL, persists)
 * - Closing tabs (removes tab, switches to next, updates URL, persists)
 *
 * Components should NEVER:
 * - Call contentState directly
 * - Call window.history.pushState directly
 * - Call content/open command directly
 *
 * Instead, call ContentService methods which handle everything.
 */

import { contentState } from './ContentStateService';
import { pageState } from './PageStateService';
import { Commands } from '../core/shared/Commands';
import type { UUID } from '../core/types/CrossPlatformUUID';
import type { ContentType, ContentPriority, ContentItem } from '../data/entities/UserStateEntity';
import type { ContentOpenParams, ContentOpenResult } from '../../commands/collaboration/content/open/shared/ContentOpenTypes';
import type { StateContentSwitchParams, StateContentSwitchResult } from '../../commands/state/content/switch/shared/StateContentSwitchTypes';
import type { StateContentCloseParams, StateContentCloseResult } from '../../commands/state/content/close/shared/StateContentCloseTypes';

export interface OpenContentOptions {
  title?: string;
  subtitle?: string;
  uniqueId?: string;
  priority?: ContentPriority;
  metadata?: Record<string, unknown>;
  setAsCurrent?: boolean;
}

class ContentServiceImpl {
  private currentUserId: UUID | null = null;

  /**
   * Set the current user ID (call once on app init)
   */
  setUserId(userId: UUID): void {
    this.currentUserId = userId;
  }

  /**
   * Open content - THE entry point for all content opening
   *
   * This method:
   * 1. Adds tab optimistically (instant UI)
   * 2. Updates pageState (view switches)
   * 3. Updates URL (browser history)
   * 4. Persists to server (background)
   */
  open(contentType: ContentType | string, entityId?: string, options: OpenContentOptions = {}): ContentItem {
    const {
      title = this.deriveTitle(contentType),
      subtitle,
      uniqueId = entityId,
      priority = 'normal',
      metadata,
      setAsCurrent = true
    } = options;

    // 1. OPTIMISTIC: Add tab immediately
    const newItem = contentState.addItem({
      type: contentType as ContentType,
      entityId: entityId as UUID | undefined,
      uniqueId,
      title,
      subtitle,
      priority,
      metadata
    }, setAsCurrent);

    // 2. Update pageState (triggers view switch in MainWidget)
    if (setAsCurrent) {
      // Only pass resolved entity if we have all required fields
      const resolved = entityId && uniqueId && title ? {
        id: entityId as UUID,
        uniqueId,
        displayName: title
      } : undefined;
      pageState.setContent(contentType, entityId, resolved);
    }

    // 3. Update URL
    if (setAsCurrent) {
      this.updateUrl(contentType, uniqueId);
    }

    // 4. Persist to server (background, non-blocking)
    if (this.currentUserId) {
      Commands.execute<ContentOpenParams, ContentOpenResult>('collaboration/content/open', {
        userId: this.currentUserId,
        contentType: contentType as ContentType,
        entityId: entityId as UUID,
        title,
        subtitle,
        priority,
        metadata,
        setAsCurrent
      }).then(result => {
        // Update temp ID with real ID from server
        if (result?.contentItemId && newItem.id.startsWith('temp-')) {
          contentState.updateItemId(newItem.id, result.contentItemId);
        }
      }).catch(err => {
        console.error(`ContentService: Failed to persist open for ${contentType}:`, err);
      });
    }

    return newItem;
  }

  /**
   * Switch to an existing tab
   */
  switchTo(tabId: string): boolean {
    const item = contentState.openItems.find(i => i.id === tabId);
    if (!item) return false;

    // Already current?
    if (contentState.currentItemId === tabId) return true;

    // 1. Update contentState
    contentState.setCurrent(tabId as UUID);

    // 2. Update pageState
    const resolved = item.entityId && item.uniqueId && item.title ? {
      id: item.entityId as UUID,
      uniqueId: item.uniqueId,
      displayName: item.title
    } : undefined;
    pageState.setContent(item.type, item.entityId, resolved);

    // 3. Update URL
    this.updateUrl(item.type, item.uniqueId || item.entityId);

    // 4. Persist to server (background)
    if (this.currentUserId) {
      Commands.execute<StateContentSwitchParams, StateContentSwitchResult>('state/content/switch', {
        userId: this.currentUserId,
        contentItemId: tabId as UUID
      }).catch(err => {
        console.error('ContentService: Failed to persist switch:', err);
      });
    }

    return true;
  }

  /**
   * Close a tab
   */
  close(tabId: string): void {
    const wasCurrentItem = contentState.currentItemId === tabId;

    // 1. Remove from contentState
    contentState.removeItem(tabId as UUID);

    // 2. If was current, switch to new current
    const newCurrent = contentState.currentItem;
    if (wasCurrentItem && newCurrent) {
      const resolved = newCurrent.entityId && newCurrent.uniqueId && newCurrent.title ? {
        id: newCurrent.entityId as UUID,
        uniqueId: newCurrent.uniqueId,
        displayName: newCurrent.title
      } : undefined;
      pageState.setContent(newCurrent.type, newCurrent.entityId, resolved);
      this.updateUrl(newCurrent.type, newCurrent.uniqueId || newCurrent.entityId);
    }

    // 3. Persist to server (background)
    if (this.currentUserId) {
      Commands.execute<StateContentCloseParams, StateContentCloseResult>('state/content/close', {
        userId: this.currentUserId,
        contentItemId: tabId as UUID
      }).catch(err => {
        console.error('ContentService: Failed to persist close:', err);
      });
    }
  }

  /**
   * Update browser URL - ONLY called from this service
   */
  private updateUrl(contentType: string, identifier?: string): void {
    const newPath = identifier ? `/${contentType}/${identifier}` : `/${contentType}`;
    if (window.location.pathname !== newPath) {
      window.history.pushState({ path: newPath }, '', newPath);
    }
  }

  /**
   * Derive title from content type
   */
  private deriveTitle(contentType: string): string {
    return contentType
      .split('-')
      .map(word => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');
  }

  /**
   * Get current state (for debugging)
   */
  getState() {
    return {
      userId: this.currentUserId,
      currentItemId: contentState.currentItemId,
      openItemsCount: contentState.openItems.length,
      currentPath: window.location.pathname
    };
  }
}

// Singleton export
export const ContentService = new ContentServiceImpl();
