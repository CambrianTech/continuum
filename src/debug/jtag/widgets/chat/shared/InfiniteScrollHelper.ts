/**
 * Infinite Scroll Helper for Chat Messages
 *
 * Combines cursor-based pagination with intersection observer
 * for efficient loading of chat history
 */

import type { ChatMessageData } from '../../../system/data/domains/ChatMessage';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';

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
  private sentinel?: HTMLElement;
  private scrollContainer?: Element;

  constructor(options: Partial<InfiniteScrollOptions> = {}) {
    this.options = {
      pageSize: 20,
      threshold: 0.1,
      ...options
    };
    console.log('🔧 CLAUDE-DEPLOY-' + Date.now() + ': InfiniteScrollHelper constructor - fewer messages fix deployed');
  }

  /**
   * Initialize intersection observer for a scroll container
   */
  setupIntersectionObserver(
    scrollContainer: Element,
    loadMoreCallback: (cursor: string) => Promise<ChatMessageData[]>
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

    this.observer.observe(this.sentinel);
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

    if (!this.loadMoreCallback) {
      console.log('❌ InfiniteScrollHelper: Missing callback, aborting');
      return;
    }

    if (!this.state.oldestTimestamp) {
      console.log('❌ InfiniteScrollHelper: Missing oldestTimestamp, aborting');
      console.log('🔧 This probably means initializeWithMessages was never called or got empty messages');
      return;
    }

    this.state = { ...this.state, isLoading: true };
    console.log('🔄 InfiniteScrollHelper: Loading messages with cursor:', this.state.oldestTimestamp);

    try {
      const newMessages = await this.loadMoreCallback(this.state.oldestTimestamp!);
      console.log('✅ InfiniteScrollHelper: Loaded', newMessages.length, 'new messages');

      // Stop loading if we get 0 messages OR fewer than requested (reached end of data)
      if (newMessages.length === 0 || newMessages.length < this.options.pageSize) {
        console.log('🔚 InfiniteScrollHelper: Reached end of data - got', newMessages.length, 'messages, expected', this.options.pageSize);
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
        console.log('📊 InfiniteScrollHelper: Updated cursor to:', oldestMessage.timestamp);
        console.log('🔧 CLAUDE-STATE-BEFORE:', this.state.oldestTimestamp);
        this.state = {
          ...this.state,
          oldestTimestamp: oldestMessage.timestamp,
          isLoading: false
        };
        console.log('🔧 CLAUDE-STATE-AFTER:', this.state.oldestTimestamp);
      }
    } catch (error) {
      console.error('❌ InfiniteScrollHelper: Failed to load more messages:', error);
      this.state = { ...this.state, isLoading: false };
    }
  }

  /**
   * Force intersection observer to re-evaluate after DOM changes
   * Uses DOM events for proper timing with Shadow DOM
   */
  forceIntersectionCheck(): void {
    if (this.sentinel && this.scrollContainer && this.observer) {
      console.log('🔧 InfiniteScrollHelper: Forcing intersection check after DOM update');

      // Use requestAnimationFrame to ensure DOM updates are complete
      requestAnimationFrame(() => {
        if (this.sentinel && this.scrollContainer) {
          // Remove and re-add sentinel to force intersection observer update
          this.sentinel.remove();
          this.scrollContainer.insertBefore(this.sentinel, this.scrollContainer.firstChild);
          console.log('🔧 InfiniteScrollHelper: Repositioned sentinel after DOM frame');

          // Use another requestAnimationFrame for intersection observer timing
          requestAnimationFrame(() => {
            if (this.observer && this.sentinel) {
              this.observer.unobserve(this.sentinel);
              this.observer.observe(this.sentinel);
              console.log('🔧 InfiniteScrollHelper: Re-observed sentinel after repositioning');
            }
          });
        }
      });
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

      // ALWAYS assume there's more data unless server tells us otherwise (by returning 0 messages)
      const newState = {
        hasMore: true,
        isLoading: false,
        oldestTimestamp: sortedMessages[sortedMessages.length - 1].timestamp,
        newestTimestamp: sortedMessages[0].timestamp
      };

      console.log('🔧 CLAUDE-DEBUG-' + Date.now() + ': Setting cursor state', {
        pageSize: this.options.pageSize,
        messageCount: messages.length,
        assumingMore: true, // Always assume more until proven otherwise
        oldestTimestamp: newState.oldestTimestamp,
        newestTimestamp: newState.newestTimestamp
      });

      this.state = newState;
      console.log('✅ InfiniteScrollHelper: State initialized:', this.state);
    } else {
      console.log('⚠️ InfiniteScrollHelper: No messages to initialize with');
    }
  }

  /**
   * Build cursor-based query parameters for loading older messages
   */
  getCursorQueryParams(roomId: string): DataListParams {
    console.log('🔧 CLAUDE-DEBUG-' + Date.now() + ': getCursorQueryParams called', {
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