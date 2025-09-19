/**
 * Abstract Message Content Adapter
 *
 * Next-generation AI-driven, dynamically editable web component pattern
 * Each content type (text, image, url_card, etc.) gets its own adapter
 * that handles rendering, interaction, and lifecycle independently.
 */

import type { ChatMessageData } from '../../../system/data/domains/ChatMessage';
import type { ChatContentType } from '../shared/ChatMessagePayload';

export interface AdapterRenderOptions {
  readonly enableIntersectionObserver?: boolean;
  readonly lazyLoadContent?: boolean;
  readonly enableInteractions?: boolean;
  readonly customClassNames?: ReadonlyArray<string>;
  readonly aiEditingEnabled?: boolean; // Future: AI can edit this content type
}

export interface AdapterLifecycleHooks {
  onContentReady?: () => void;
  onContentError?: (error: Error) => void;
  onUserInteraction?: (interaction: string, data: any) => void;
  onAIEdit?: (editData: any) => void; // Future: AI editing capability
}

/**
 * Abstract base for all message content adapters
 * Follows React/Next.js component patterns with strong typing
 */
export abstract class AbstractMessageAdapter<TContentData = any> {
  protected readonly contentType: ChatContentType;
  protected readonly options: AdapterRenderOptions;
  protected readonly hooks: AdapterLifecycleHooks;
  protected contentData?: TContentData;

  constructor(
    contentType: ChatContentType,
    options: AdapterRenderOptions = {},
    hooks: AdapterLifecycleHooks = {}
  ) {
    this.contentType = contentType;
    this.options = {
      enableIntersectionObserver: false,
      lazyLoadContent: true,
      enableInteractions: true,
      aiEditingEnabled: false,
      ...options
    };
    this.hooks = hooks;
  }

  /**
   * Parse message content into typed data structure
   * Each adapter implements its own content parsing logic
   */
  abstract parseContent(message: ChatMessageData): TContentData | null;

  /**
   * Render the content HTML
   * Each adapter handles its own rendering with full control
   */
  abstract renderContent(data: TContentData, currentUserId: string): string;

  /**
   * Handle content loading (images, embeds, etc.)
   * Each adapter manages its own async content lifecycle
   */
  abstract handleContentLoading(element: HTMLElement): Promise<void>;

  /**
   * Get CSS classes specific to this content type
   */
  abstract getContentClasses(): string[];

  /**
   * Get CSS for this content type (injected once into chat widget's shadow DOM)
   * Each adapter provides styles that get added to the main stylesheet
   */
  abstract getCSS(): string;

  /**
   * Static method to inject all adapter CSS into chat widget's shadow DOM
   * Called once during chat widget initialization, not per-row
   */
  static injectAdapterStyles(shadowRoot: ShadowRoot, adapters: AbstractMessageAdapter[]): void {
    const styleId = 'message-adapter-styles';
    let styleEl = shadowRoot.querySelector(`#${styleId}`) as HTMLStyleElement;

    if (!styleEl) {
      styleEl = document.createElement('style');
      styleEl.id = styleId;
      shadowRoot.appendChild(styleEl);
    }

    // Combine all adapter CSS
    const combinedCSS = adapters.map(adapter => adapter.getCSS()).join('\n');
    styleEl.textContent = combinedCSS;
  }

  /**
   * Main render method - just returns HTML, no per-row CSS injection
   * Efficient for dynamic paging/infinite scroll
   */
  renderMessage(message: ChatMessageData, currentUserId: string): string {
    try {
      // Parse content using adapter-specific logic
      this.contentData = this.parseContent(message) || undefined;
      if (!this.contentData) {
        return this.renderError('Unable to parse content');
      }

      // Just render HTML - CSS already injected into shadow DOM
      const contentHtml = this.renderContent(this.contentData, currentUserId);
      const classes = [
        'message-content-adapter',
        `content-type-${this.contentType}`,
        ...this.getContentClasses(),
        ...(this.options.customClassNames || [])
      ].join(' ');

      return `<div class="${classes}" data-content-type="${this.contentType}">${contentHtml}</div>`;
    } catch (error) {
      console.error(`Adapter ${this.contentType} render error:`, error);
      return this.renderError('Content rendering failed');
    }
  }

  /**
   * Post-render initialization (called after DOM insertion)
   * Efficiently handles new rows without re-processing existing content
   * Used for both infinite scroll paging and real-time message insertion
   */
  async initializeInDOM(element: HTMLElement): Promise<void> {
    try {
      // Skip initialization if already processed (for efficiency)
      if (element.dataset.initialized === 'true') {
        return;
      }

      // Mark as processing to prevent duplicate initialization
      element.dataset.initialized = 'processing';

      // Set up intersection observer if enabled
      if (this.options.enableIntersectionObserver) {
        this.setupIntersectionObserver(element);
      }

      // Handle async content loading (images, embeds, etc.)
      await this.handleContentLoading(element);

      // Set up interactions if enabled
      if (this.options.enableInteractions) {
        this.setupInteractionHandlers(element);
      }

      // Mark as fully initialized
      element.dataset.initialized = 'true';

      // Notify that content is ready
      this.hooks.onContentReady?.();
    } catch (error) {
      console.error(`Adapter ${this.contentType} initialization error:`, error);
      element.dataset.initialized = 'error';
      this.hooks.onContentError?.(error as Error);
    }
  }

  /**
   * Batch initialize multiple new rows efficiently
   * Used when infinite scroll loads multiple messages at once
   */
  static async batchInitializeRows(
    elements: HTMLElement[],
    adapters: Map<string, AbstractMessageAdapter>
  ): Promise<void> {
    const initPromises: Promise<void>[] = [];

    for (const element of elements) {
      const contentType = element.dataset.contentType;
      const adapter = adapters.get(contentType || 'text');

      if (adapter) {
        initPromises.push(adapter.initializeInDOM(element));
      }
    }

    // Initialize all rows in parallel for better performance
    await Promise.all(initPromises);
  }

  /**
   * Error rendering fallback
   */
  protected renderError(message: string): string {
    return `<div class="content-error" data-content-type="${this.contentType}">
      <span class="error-icon">⚠️</span>
      <span class="error-message">${message}</span>
    </div>`;
  }

  /**
   * Set up intersection observer for lazy loading
   */
  protected setupIntersectionObserver(element: HTMLElement): void {
    // Future: Implement lazy loading with intersection observer
    console.log(`🔄 Setting up intersection observer for ${this.contentType}`);
  }

  /**
   * Set up user interaction handlers
   */
  protected setupInteractionHandlers(element: HTMLElement): void {
    if (!this.options.enableInteractions) return;

    element.addEventListener('click', (e) => {
      this.hooks.onUserInteraction?.('click', {
        contentType: this.contentType,
        target: e.target,
        contentData: this.contentData
      });
    });
  }

  /**
   * Future: AI editing capabilities
   * Each adapter can define what AI can edit about its content
   */
  protected getAIEditableFields(): Record<string, any> {
    return {}; // Override in subclasses
  }

  /**
   * Future: Handle AI-driven content editing
   */
  async handleAIEdit(editInstructions: any): Promise<void> {
    if (!this.options.aiEditingEnabled) return;

    console.log(`🤖 AI editing ${this.contentType}:`, editInstructions);
    this.hooks.onAIEdit?.(editInstructions);
  }
}