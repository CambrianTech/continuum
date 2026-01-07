/**
 * Infinite Scroll Helper for Chat Messages
 *
 * Combines cursor-based pagination with intersection observer
 * for efficient loading of chat history
 */

import { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

export interface CursorPaginationState {
  readonly hasMore: boolean;
  readonly isLoading: boolean;
  readonly oldestTimestamp?: Date; // Cursor for loading older messages
  readonly newestTimestamp?: Date; // Cursor for loading newer messages
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
  private loadMoreCallback?: (cursor: Date) => Promise<ChatMessageEntity[]>;
  private sentinel?: HTMLElement;
  private scrollContainer?: Element;

  constructor(options: Partial<InfiniteScrollOptions> = {}) {
    this.options = {
      pageSize: 20,
      threshold: 0.1,
      ...options
    };
    verbose() && console.log('🔧 CLAUDE-DEPLOY-' + Date.now() + ': InfiniteScrollHelper constructor - fewer messages fix deployed');
  }

  /**
   * Initialize intersection observer for a scroll container
   */
  setupIntersectionObserver(
    scrollContainer: Element,
    loadMoreCallback: (cursor: Date) => Promise<ChatMessageEntity[]>
  ): void {
    this.loadMoreCallback = loadMoreCallback;
    this.scrollContainer = scrollContainer;

    // Create sentinel element at top of container to detect scroll to top
    this.sentinel = document.createElement('div');
    this.sentinel.className = 'infinite-scroll-sentinel';
    this.sentinel.style.height = '1px';
    this.sentinel.style.visibility = 'hidden';

    this.scrollContainer.insertBefore(this.sentinel, this.scrollContainer.firstChild);

    // Set up intersection observer
    verbose() && console.log('🔄 InfiniteScrollHelper: Setting up intersection observer');
    this.observer = new IntersectionObserver(
      (entries) => {
        const entry = entries[0];
        verbose() && console.log('👁️ InfiniteScrollHelper: Intersection observed:', {
          isIntersecting: entry.isIntersecting,
          canLoadMore: this.canLoadMore(),
          intersectionRatio: entry.intersectionRatio
        });
        if (entry.isIntersecting && this.canLoadMore()) {
          verbose() && console.log('✅ InfiniteScrollHelper: Triggering loadOlderMessages');
          this.loadOlderMessages();
        }
      },
      {
        root: scrollContainer,
        rootMargin: `${this.options.threshold * 100}% 0px`,
        threshold: 0
      }
    );

    this.observer.observe(this.sentinel);
  }

  /**
   * Load older messages using cursor pagination
   */
  private async loadOlderMessages(): Promise<void> {
    verbose() && console.log('🔄 InfiniteScrollHelper: loadOlderMessages triggered');
    verbose() && console.log('📊 Current state:', {
      hasCallback: !!this.loadMoreCallback,
      oldestTimestamp: this.state.oldestTimestamp,
      isLoading: this.state.isLoading,
      hasMore: this.state.hasMore
    });

    if (!this.loadMoreCallback) {
      verbose() && console.log('❌ InfiniteScrollHelper: Missing callback, aborting');
      return;
    }

    if (!this.state.oldestTimestamp) {
      verbose() && console.log('❌ InfiniteScrollHelper: Missing oldestTimestamp, aborting');
      verbose() && console.log('🔧 This probably means initializeWithMessages was never called or got empty messages');
      return;
    }

    this.state = { ...this.state, isLoading: true };
    verbose() && console.log('🔄 InfiniteScrollHelper: Loading messages with cursor:', this.state.oldestTimestamp);

    try {
      const newMessages = await this.loadMoreCallback(this.state.oldestTimestamp!);
      verbose() && console.log('✅ InfiniteScrollHelper: Loaded', newMessages.length, 'new messages');

      // Stop loading if we get 0 messages OR fewer than requested (reached end of data)
      if (newMessages.length === 0 || newMessages.length < this.options.pageSize) {
        verbose() && console.log('🔚 InfiniteScrollHelper: Reached end of data - got', newMessages.length, 'messages, expected', this.options.pageSize);
        this.state = {
          ...this.state,
          hasMore: false,
          isLoading: false,
          // Still update cursor if we got some messages
          oldestTimestamp: newMessages.length > 0 ? newMessages[0].timestamp : this.state.oldestTimestamp
        };
      } else {
        // Update cursor to oldest message timestamp
        // newMessages is in chronological order (oldest first) after ChatWidget's reverse()
        const oldestMessage = newMessages[0];
        verbose() && console.log('📊 InfiniteScrollHelper: Updated cursor to:', oldestMessage.timestamp);
        verbose() && console.log('🔧 CLAUDE-STATE-BEFORE:', this.state.oldestTimestamp);
        this.state = {
          ...this.state,
          oldestTimestamp: oldestMessage.timestamp,
          isLoading: false
        };
        verbose() && console.log('🔧 CLAUDE-STATE-AFTER:', this.state.oldestTimestamp);
      }
    } catch (error) {
      console.error('❌ InfiniteScrollHelper: Failed to load more messages:', error);
      this.state = { ...this.state, isLoading: false };
    }
  }

  /**
   * Force intersection observer to re-evaluate after DOM changes
   * DOM is already updated synchronously - no RAF needed
   */
  forceIntersectionCheck(): void {
    if (this.sentinel && this.scrollContainer && this.observer) {
      verbose() && console.log('🔧 InfiniteScrollHelper: Forcing intersection check after DOM update');

      // DOM is already updated - remove/re-add sentinel immediately
      this.sentinel.remove();
      this.scrollContainer.insertBefore(this.sentinel, this.scrollContainer.firstChild);
      verbose() && console.log('🔧 InfiniteScrollHelper: Repositioned sentinel');

      // Reset observer immediately - no RAF needed
      this.observer.unobserve(this.sentinel);
      this.observer.observe(this.sentinel);
      verbose() && console.log('🔧 InfiniteScrollHelper: Re-observed sentinel');
    }
  }

  /**
   * Initialize pagination state with first batch of messages
   */
  initializeWithMessages(messages: ChatMessageEntity[]): void {
    verbose() && console.log('🔄 InfiniteScrollHelper: initializeWithMessages called with', messages.length, 'messages');
    if (messages.length > 0) {
      const sortedMessages = [...messages].sort((a, b) =>
        new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
      );

      verbose() && console.log('📊 InfiniteScrollHelper: Sorted messages by timestamp');
      verbose() && console.log('📊 Newest timestamp:', sortedMessages[0].timestamp);
      verbose() && console.log('📊 Oldest timestamp:', sortedMessages[sortedMessages.length - 1].timestamp);

      // ALWAYS assume there's more data unless server tells us otherwise (by returning 0 messages)
      const newState = {
        hasMore: true,
        isLoading: false,
        oldestTimestamp: sortedMessages[sortedMessages.length - 1].timestamp,
        newestTimestamp: sortedMessages[0].timestamp
      };

      verbose() && console.log('🔧 CLAUDE-DEBUG-' + Date.now() + ': Setting cursor state', {
        pageSize: this.options.pageSize,
        messageCount: messages.length,
        assumingMore: true, // Always assume more until proven otherwise
        oldestTimestamp: newState.oldestTimestamp,
        newestTimestamp: newState.newestTimestamp
      });

      this.state = newState;
      verbose() && console.log('✅ InfiniteScrollHelper: State initialized:', this.state);
    } else {
      verbose() && console.log('⚠️ InfiniteScrollHelper: No messages to initialize with');
    }
  }

  /**
   * Build cursor-based query parameters for loading older messages
   */
  getCursorQueryParams(roomId: string): DataListParams {
    verbose() && console.log('🔧 CLAUDE-DEBUG-' + Date.now() + ': getCursorQueryParams called', {
      roomId: roomId,
      oldestTimestamp: this.state.oldestTimestamp,
      hasMore: this.state.hasMore,
      isLoading: this.state.isLoading
    });

    return {
      collection: ChatMessageEntity.collection,
      filter: { roomId },
      orderBy: [{ field: 'timestamp', direction: 'desc' }], // DESC to get messages before cursor
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