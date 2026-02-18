/**
 * Base Message Row Widget - Modular Chat Message Rendering
 *
 * Provides common message row functionality (positioning, timestamps, reactions)
 * while allowing specialized content rendering based on message type.
 *
 * Architecture: Content Type → Widget Plugin Mapping
 * Each ChatContentType gets its own specialized renderer that extends this base.
 */

import { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import { ChatMessageEntityHelpers } from './ChatModuleTypes';
import type { ChatMessagePayload, ChatContentType } from './ChatMessagePayload';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && (window as any).JTAG_VERBOSE === true;

/**
 * Message Renderer Interface - Extensible for future widget conversion
 * Designed with intersection observer support in mind for lazy loading
 */
export interface MessageRendererOptions {
  readonly enableIntersectionObserver?: boolean;
  readonly lazyLoadImages?: boolean;
  readonly enableInteractions?: boolean;
  readonly customClassNames?: ReadonlyArray<string>;
}

/**
 * Message Renderer State - For future stateful widget conversion
 */
export interface MessageRendererState {
  readonly isVisible?: boolean;
  readonly isLoading?: boolean;
  readonly hasError?: boolean;
  readonly interactionCount?: number;
}

/**
 * Base message renderer - Well-typed, extensible architecture
 * Future: Convert to BaseWidget extensions with intersection observer
 */
export abstract class BaseMessageRowWidget {
  protected readonly options: MessageRendererOptions;
  protected state: MessageRendererState = {};
  
  constructor(options: MessageRendererOptions = {}) {
    this.options = {
      enableIntersectionObserver: false,
      lazyLoadImages: true,
      enableInteractions: true,
      customClassNames: [],
      ...options
    };
  }
  
  /**
   * Abstract method for specialized content rendering
   * Each message type implements this differently
   * Future: May return Promise<string> for async widget rendering
   */
  abstract renderContent(message: ChatMessageEntity): string;
  
  /**
   * Abstract method for content type validation
   * Ensures type safety and proper renderer selection
   */
  abstract canRender(message: ChatMessageEntity): boolean;
  
  /**
   * Hook for future intersection observer integration
   * Called when message becomes visible in viewport
   */
  protected onMessageVisible(message: ChatMessageEntity): void {
    this.state = { ...this.state, isVisible: true };
  }
  
  /**
   * Hook for future interaction handling
   * Called when user interacts with rendered message
   */
  protected onMessageInteraction(message: ChatMessageEntity, interactionType: string): void {
    this.state = { 
      ...this.state, 
      interactionCount: (this.state.interactionCount || 0) + 1 
    };
  }

  /**
   * Main message container with common features:
   * - Me/someone-else positioning (right/left alignment)
   * - Message bubble styling
   * - Timestamp display
   * - Reaction system
   */
  public renderMessageContainer(message: ChatMessageEntity, currentUserId: string): string {
    // Use semantic helper methods for clean, explicit logic
    const isCurrentUser = ChatMessageEntityHelpers.isFromCurrentUser(message, currentUserId);
    const alignment = ChatMessageEntityHelpers.getAlignment(message, currentUserId);
    const userClass = ChatMessageEntityHelpers.getUserPositionClass(message, currentUserId);
    const displayName = ChatMessageEntityHelpers.getDisplayName(message);

    verbose() && console.log(`🔧 CLAUDE-RENDER-DEBUG: senderId="${message.senderId}", currentUserId="${currentUserId}", isCurrentUser=${isCurrentUser}, alignment="${alignment}"`);

    return `
      <div class="message-row ${alignment}">
        <div class="message-bubble ${userClass}">
          <div class="message-header">
            ${!isCurrentUser ? `<span class="sender-name">${displayName}</span>` : ''}
            <span class="message-time">${this.formatTimestamp(message.timestamp)}</span>
          </div>
          <div class="message-content">
            ${this.renderContent(message)}
          </div>
          ${this.renderReactions(message)}
          ${this.renderMessageStatus(message)}
        </div>
      </div>
    `;
  }

  /**
   * Format timestamp for display - TEMP: showing full date/time for debugging chronological order
   */
  private formatTimestamp(timestamp: Date | string): string {
    try {
      const date = timestamp instanceof Date ? timestamp : new Date(timestamp);
      // TEMP DEBUG: Show full date and time to verify chronological ordering
      return `${date.toLocaleDateString()} ${date.toLocaleTimeString([], {
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit'
      })}`;
    } catch {
      return 'Unknown time';
    }
  }

  /**
   * Render reaction system (if message has reactions)
   */
  private renderReactions(message: ChatMessageEntity): string {
    // For future ChatMessageDataPayload integration
    // const payload = message as unknown as ChatMessageDataPayload;
    // if (payload.reactions && payload.reactions.length > 0) {
    //   return `<div class="reactions">${payload.reactions.map(r => 
    //     `<span class="reaction">${r.emoji} ${r.count}</span>`
    //   ).join('')}</div>`;
    // }
    return '';
  }

  /**
   * Render message status (sending, sent, delivered, error)
   */
  private renderMessageStatus(message: ChatMessageEntity): string {
    if (message.status && message.status !== 'sent') {
      const statusIcon: Record<string, string> = {
        'sending': '⏳',
        'delivered': '✓✓',
        'read': '✓✓',
        'failed': '❌',
        'deleted': '🗑️'
      };

      return `<div class="message-status">${statusIcon[message.status] || ''}</div>`;
    }
    return '';
  }
}

/**
 * Message Renderer Registry - Content Type → Widget Plugin Mapping
 * Future: Support widget creation with BaseWidget integration
 */
export type MessageRendererRegistry = Record<ChatContentType, new(options?: MessageRendererOptions) => BaseMessageRowWidget>;

/**
 * Future Widget Renderer Registry - For BaseWidget conversion
 * Will use intersection observer for performance optimization
 */
export type WidgetMessageRendererRegistry = Record<ChatContentType, {
  readonly rendererClass: new(options?: MessageRendererOptions) => BaseMessageRowWidget;
  readonly widgetClass?: new(message: ChatMessageEntity) => any; // Future BaseWidget extension
  readonly requiresIntersectionObserver?: boolean;
  readonly supportsLazyLoading?: boolean;
}>;

/**
 * Default Text Message Renderer - Well-typed with validation
 * Future: Convert to TextMessageWidget extending BaseWidget
 */
export class TextMessageRowWidget extends BaseMessageRowWidget {
  constructor(options: MessageRendererOptions = {}) {
    super({
      enableIntersectionObserver: true,
      lazyLoadImages: false, // Text messages don't have images
      enableInteractions: true,
      customClassNames: ['text-message-renderer'],
      ...options
    });
  }

  canRender(message: ChatMessageEntity): boolean {
    if (!message.content) {
      throw new Error('TextMessageRowWidget.canRender: message.content is required');
    }
    if (typeof message.content.text !== 'string') {
      throw new Error(`TextMessageRowWidget.canRender: message.content.text must be string, got ${typeof message.content.text}`);
    }
    return message.content.text.trim().length > 0;
  }

  renderContent(message: ChatMessageEntity): string {
    if (!this.canRender(message)) {
      throw new Error('TextMessageRowWidget.renderContent: message failed canRender check');
    }

    const customClasses = this.options.customClassNames?.join(' ') || '';
    const content = this.escapeHtml(message.content.text); // Keep original formatting
    const interactionAttrs = this.options.enableInteractions
      ? 'data-interactive="true" tabindex="0"'
      : '';

    return `<p class="text-content ${customClasses}" ${interactionAttrs}>${content}</p>`;
  }

  private escapeHtml(text: string): string {
    // Safe HTML escaping without DOM manipulation
    return text
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#039;');
  }
}

/**
 * Future Image Message Renderer - Prepared for intersection observer
 */
export class ImageMessageRowWidget extends BaseMessageRowWidget {
  constructor(options: MessageRendererOptions = {}) {
    super({
      enableIntersectionObserver: true, // Critical for image lazy loading
      lazyLoadImages: true,
      enableInteractions: true,
      customClassNames: ['image-message-renderer'],
      ...options
    });
  }

  canRender(message: ChatMessageEntity): boolean {
    // Future: Check for image content type in ChatMessageDataPayload
    return message.content.text.includes('http') &&
           (message.content.text.includes('.jpg') || message.content.text.includes('.png') ||
            message.content.text.includes('.gif') || message.content.text.includes('.webp'));
  }

  renderContent(message: ChatMessageEntity): string {
    if (!this.canRender(message)) {
      return '<p class="text-content error">Invalid image content</p>';
    }
    
    const customClasses = this.options.customClassNames?.join(' ') || '';
    const lazyAttrs = this.options.lazyLoadImages 
      ? 'loading="lazy" data-intersection-target="true"'
      : '';
    const interactionAttrs = this.options.enableInteractions 
      ? 'data-interactive="true" tabindex="0"'
      : '';
    
    return `
      <div class="image-content ${customClasses}" ${interactionAttrs}>
        <img src="${this.escapeHtml(message.content.text)}"
             alt="Shared image"
             ${lazyAttrs}
             class="message-image" />
      </div>
    `;
  }
  
  private escapeHtml(text: string): string {
    return text
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#039;');
  }
}

/**
 * Factory for creating appropriate message renderer
 */
/**
 * Factory for creating well-typed message renderers
 * Future: Support widget options and intersection observer configuration
 */
export class MessageRowWidgetFactory {
  private static readonly renderers: Record<string, new(options?: MessageRendererOptions) => BaseMessageRowWidget> = {
    'text': TextMessageRowWidget,
    'image': ImageMessageRowWidget,
  };
  
  /**
   * Type-safe renderer selection with strong typing
   */
  static createRenderer(
    message: ChatMessageEntity,
    options: MessageRendererOptions = {}
  ): BaseMessageRowWidget {
    if (!message) {
      throw new Error('MessageRowWidgetFactory.createRenderer: message is required');
    }
    if (!message.content) {
      throw new Error('MessageRowWidgetFactory.createRenderer: message.content is required');
    }
    if (typeof message.content.text !== 'string') {
      throw new Error(`MessageRowWidgetFactory.createRenderer: message.content.text must be string, got ${typeof message.content.text}`);
    }

    // Strong type-safe content type detection
    let contentType: ChatContentType = 'text';

    const messageText = message.content.text;
    if (messageText.includes('http') &&
        (messageText.includes('.jpg') || messageText.includes('.png') ||
         messageText.includes('.gif') || messageText.includes('.webp'))) {
      contentType = 'image';
    }

    // Type-safe renderer selection
    const RendererClass = this.renderers[contentType];
    if (!RendererClass) {
      throw new Error(`MessageRowWidgetFactory.createRenderer: No renderer found for content type "${contentType}"`);
    }
    return new RendererClass(options);
  }
  
  /**
   * Register new message renderer types
   * Type-safe registration with validation
   */
  static registerRenderer<T extends BaseMessageRowWidget>(
    contentType: ChatContentType | string, 
    rendererClass: new(options?: MessageRendererOptions) => T
  ): void {
    this.renderers[contentType] = rendererClass;
  }
  
  /**
   * Get all supported content types
   */
  static getSupportedTypes(): string[] {
    return Object.keys(this.renderers);
  }
  
  /**
   * Check if a content type is supported
   */
  static supportsContentType(contentType: string): boolean {
    return contentType in this.renderers;
  }
}