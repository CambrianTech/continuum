/**
 * Infinite Scroll Helper for Chat Messages
 *
 * Combines cursor-based pagination with intersection observer
 * for efficient loading of chat history
 */

import type { ChatMessageData } from '../../../system/data/domains/ChatMessage';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';

export interface CursorPaginationState {
  readonly hasMore: boolean;
  readonly isLoading: boolean;
  readonly oldestTimestamp?: string; // Cursor for loading older messages
  readonly newestTimestamp?: string; // Cursor for loading newer messages
}

export interface InfiniteScrollOptions {
  readonly pageSize: number;
  readonly threshold: number; // How close to top/bottom to trigger loading
}

/**
 * Helper class for managing infinite scroll with cursor pagination
 */
export class InfiniteScrollHelper {
  private options: InfiniteScrollOptions;
  private state: CursorPaginationState = {
    hasMore: true,
    isLoading: false
  };

  private observer?: IntersectionObserver;
  private loadMoreCallback?: (cursor: string) => Promise<ChatMessageData[]>;

  constructor(options: Partial<InfiniteScrollOptions> = {}) {
    this.options = {
      pageSize: 20,
      threshold: 0.1,
      ...options
    };
  }

  /**
   * Initialize intersection observer for a scroll container
   */
  setupIntersectionObserver(
    scrollContainer: Element,
    loadMoreCallback: (cursor: string) => Promise<ChatMessageData[]>
  ): void {
    this.loadMoreCallback = loadMoreCallback;

    // Create sentinel element at top of container to detect scroll to top
    const sentinel = document.createElement('div');
    sentinel.className = 'infinite-scroll-sentinel';
    sentinel.style.height = '1px';
    sentinel.style.visibility = 'hidden';

    scrollContainer.insertBefore(sentinel, scrollContainer.firstChild);

    // Set up intersection observer
    this.observer = new IntersectionObserver(
      (entries) => {
        const entry = entries[0];
        if (entry.isIntersecting && this.canLoadMore()) {
          this.loadOlderMessages();
        }
      },
      {
        root: scrollContainer,
        rootMargin: `${this.options.threshold * 100}% 0px`,
        threshold: 0
      }
    );

    this.observer.observe(sentinel);
  }

  /**
   * Load older messages using cursor pagination
   */
  private async loadOlderMessages(): Promise<void> {
    if (!this.loadMoreCallback || !this.state.oldestTimestamp) {
      return;
    }

    this.state = { ...this.state, isLoading: true };

    try {
      const newMessages = await this.loadMoreCallback(this.state.oldestTimestamp!);

      if (newMessages.length === 0) {
        this.state = { ...this.state, hasMore: false, isLoading: false };
      } else {
        // Update cursor to oldest message timestamp
        const oldestMessage = newMessages[newMessages.length - 1];
        this.state = {
          ...this.state,
          oldestTimestamp: oldestMessage.timestamp,
          isLoading: false
        };
      }
    } catch (error) {
      console.error('Failed to load more messages:', error);
      this.state = { ...this.state, isLoading: false };
    }
  }

  /**
   * Initialize pagination state with first batch of messages
   */
  initializeWithMessages(messages: ChatMessageData[]): void {
    if (messages.length > 0) {
      const sortedMessages = [...messages].sort((a, b) =>
        new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
      );

      this.state = {
        hasMore: messages.length >= this.options.pageSize,
        isLoading: false,
        oldestTimestamp: sortedMessages[sortedMessages.length - 1].timestamp,
        newestTimestamp: sortedMessages[0].timestamp
      };
    }
  }

  /**
   * Build cursor-based query parameters for loading older messages
   */
  getCursorQueryParams(roomId: string): DataListParams {
    return {
      collection: 'chat_messages',
      filter: { roomId },
      orderBy: [{ field: 'timestamp', direction: 'desc' }],
      limit: this.options.pageSize,
      cursor: this.state.oldestTimestamp ? {
        field: 'timestamp',
        value: this.state.oldestTimestamp,
        direction: 'before' // Load messages older than cursor
      } : undefined,
      context: {} as any,
      sessionId: '' as any // These will be filled by the widget
    };
  }

  canLoadMore(): boolean {
    return this.state.hasMore && !this.state.isLoading;
  }

  getState(): CursorPaginationState {
    return this.state;
  }

  cleanup(): void {
    if (this.observer) {
      this.observer.disconnect();
      this.observer = undefined;
    }
  }
}