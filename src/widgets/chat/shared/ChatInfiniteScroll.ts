/**
 * Chat Infinite Scroll Adapter
 *
 * Combines ChatMessageLoader and ChatMessageRenderer with GenericInfiniteScroll
 * to provide a complete chat-specific infinite scroll solution.
 */

import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import { GenericInfiniteScroll } from '../../shared/GenericInfiniteScroll';
import type {
  InfiniteScrollConfig,
  InfiniteScrollCallbacks
} from '../../shared/InfiniteScrollTypes';
import { ChatMessageLoader } from './ChatMessageLoader';
import { ChatMessageRenderer } from './ChatMessageRenderer';

/**
 * Chat-specific infinite scroll implementation
 * Handles loading and rendering chat messages with cursor pagination
 */
export class ChatInfiniteScroll {
  private genericScroll: GenericInfiniteScroll<ChatMessageEntity, string>;
  private loader: ChatMessageLoader;
  private renderer: ChatMessageRenderer;

  constructor(
    private readonly roomId: string,
    private readonly currentUserId: string,
    private readonly executeCommand: <T>(command: string, params: any) => Promise<T>,
    config: InfiniteScrollConfig = {
      pageSize: 20,
      threshold: 0.1,
      rootMargin: '50px',
      enabled: true
    }
  ) {
    this.loader = new ChatMessageLoader(executeCommand);
    this.renderer = new ChatMessageRenderer(currentUserId);

    const callbacks: InfiniteScrollCallbacks<ChatMessageEntity, string> = {
      loadItems: (cursor, pageSize) => this.loader.loadMessages(this.roomId, cursor, pageSize),
      getCursor: (message) => this.renderer.getCursor(message),
      compareCursors: (a, b) => this.renderer.compareCursors(a, b),
      createItemElement: (message) => this.renderer.createMessageElement(message)
    };

    this.genericScroll = new GenericInfiniteScroll(config, callbacks);
  }

  /**
   * Initialize infinite scroll with container and initial messages
   */
  async initialize(
    scrollContainer: HTMLElement,
    initialMessages: ChatMessageEntity[] = []
  ): Promise<void> {
    this.genericScroll.initialize(scrollContainer, initialMessages);
  }

  /**
   * Load initial messages for the room
   */
  async loadInitialMessages(limit = 20): Promise<ChatMessageEntity[]> {
    return this.loader.loadInitialMessages(this.roomId, limit);
  }

  /**
   * Render messages to HTML string (for initial template)
   */
  renderMessages(messages: ChatMessageEntity[]): string {
    return this.renderer.renderMessages(messages);
  }

  /**
   * Create a single message element
   */
  createMessageElement(message: ChatMessageEntity): HTMLElement {
    return this.renderer.createMessageElement(message);
  }

  /**
   * Get current pagination state
   */
  getState() {
    return this.genericScroll.getState();
  }

  /**
   * Cleanup
   */
  destroy(): void {
    this.genericScroll.destroy();
  }
}

/**
 * Default chat infinite scroll configuration
 */
export const DEFAULT_CHAT_SCROLL_CONFIG: InfiniteScrollConfig = {
  pageSize: 20,
  threshold: 0.1,
  rootMargin: '50px',
  enabled: true
} as const;