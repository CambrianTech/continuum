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
    console.log('🔄 InfiniteScrollHelper: Setting up intersection observer');
    this.observer = new IntersectionObserver(
      (entries) => {
        const entry = entries[0];
        console.log('👁️ InfiniteScrollHelper: Intersection observed:', {
          isIntersecting: entry.isIntersecting,
          canLoadMore: this.canLoadMore(),
          intersectionRatio: entry.intersectionRatio
        });
        if (entry.isIntersecting && this.canLoadMore()) {
          console.log('✅ InfiniteScrollHelper: Triggering loadOlderMessages');
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
    console.log('🔄 InfiniteScrollHelper: loadOlderMessages triggered');
    console.log('📊 Current state:', {
      hasCallback: !!this.loadMoreCallback,
      oldestTimestamp: this.state.oldestTimestamp,
      isLoading: this.state.isLoading,
      hasMore: this.state.hasMore
    });

    if (!this.loadMoreCallback || !this.state.oldestTimestamp) {
      console.log('❌ InfiniteScrollHelper: Missing callback or timestamp, aborting');
      return;
    }

    this.state = { ...this.state, isLoading: true };
    console.log('🔄 InfiniteScrollHelper: Loading messages with cursor:', this.state.oldestTimestamp);

    try {
      const newMessages = await this.loadMoreCallback(this.state.oldestTimestamp!);
      console.log('✅ InfiniteScrollHelper: Loaded', newMessages.length, 'new messages');

      if (newMessages.length === 0) {
        console.log('🔚 InfiniteScrollHelper: No more messages, disabling infinite scroll');
        this.state = { ...this.state, hasMore: false, isLoading: false };
      } else {
        // Update cursor to oldest message timestamp
        const oldestMessage = newMessages[newMessages.length - 1];
        console.log('📊 InfiniteScrollHelper: Updated cursor to:', oldestMessage.timestamp);
        this.state = {
          ...this.state,
          oldestTimestamp: oldestMessage.timestamp,
          isLoading: false
        };
      }
    } catch (error) {
      console.error('❌ InfiniteScrollHelper: Failed to load more messages:', error);
      this.state = { ...this.state, isLoading: false };
    }
  }

  /**
   * Initialize pagination state with first batch of messages
   */
  initializeWithMessages(messages: ChatMessageData[]): void {
    console.log('🔄 InfiniteScrollHelper: initializeWithMessages called with', messages.length, 'messages');
    if (messages.length > 0) {
      const sortedMessages = [...messages].sort((a, b) =>
        new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
      );

      console.log('📊 InfiniteScrollHelper: Sorted messages by timestamp');
      console.log('📊 Newest timestamp:', sortedMessages[0].timestamp);
      console.log('📊 Oldest timestamp:', sortedMessages[sortedMessages.length - 1].timestamp);

      this.state = {
        hasMore: messages.length >= this.options.pageSize,
        isLoading: false,
        oldestTimestamp: sortedMessages[sortedMessages.length - 1].timestamp,
        newestTimestamp: sortedMessages[0].timestamp
      };

      console.log('✅ InfiniteScrollHelper: State initialized:', this.state);
    } else {
      console.log('⚠️ InfiniteScrollHelper: No messages to initialize with');
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