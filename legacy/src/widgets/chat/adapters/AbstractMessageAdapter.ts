/**
 * Abstract Message Content Adapter
 *
 * Next-generation AI-driven, dynamically editable web component pattern
 * Each content type (text, image, url_card, etc.) gets its own adapter
 * that handles rendering, interaction, and lifecycle independently.
 */

import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import type { ChatContentType } from '../shared/ChatMessagePayload';

// Verbose logging helper for browser
import type { JTAGWindowProperties } from '../../../system/core/types/GlobalAugmentations';

const verbose = () => typeof window !== 'undefined' && (window as Window & JTAGWindowProperties).JTAG_VERBOSE === true;

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
  onUserInteraction?: (interaction: string, data: Record<string, unknown>) => void;
  onAIEdit?: (editData: Record<string, unknown>) => void; // Future: AI editing capability
}

/**
 * Abstract base for all message content adapters
 * Follows React/Next.js component patterns with strong typing
 */
export abstract class AbstractMessageAdapter<TContentData = unknown> {
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
  abstract parseContent(message: ChatMessageEntity): TContentData | null;

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
   *
   * LEGACY PATH: returns an HTML string that the caller assigns via
   * innerHTML on a live element. Prefer overriding `renderMessageElement`
   * — it returns a constructed DOM node, doesn't blow away reactive
   * children, and keeps user-controlled text inside `.textContent`
   * rather than re-parsed HTML. Tracked in issue #1100.
   */
  renderMessage(message: ChatMessageEntity, currentUserId: string): string {
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
   * DOM-returning render path (preferred). Returns the adapter's
   * `message-content-adapter` wrapper as an HTMLElement, ready to be
   * appended to the message bubble's content slot.
   *
   * Default body (DRY — issue #1158): parse content via the subclass's
   * `parseContent`, build the wrapper via `createAdapterWrapper`, render
   * the rich content string via `renderContent`, then adopt it on a
   * detached `<template>` and append the resulting `DocumentFragment`
   * to the wrapper. The live message-content slot never sees `innerHTML`,
   * so any Lit-managed reactive children survive sibling updates.
   *
   * Subclasses only need to override this when they build the wrapper's
   * children directly via DOM APIs (e.g. `ImageMessageAdapter` constructs
   * `<img>` nodes via property assignment to keep src/alt out of any
   * HTML-parse path). Adapters that already produce a clean HTML string
   * from `renderContent` should NOT override this — the default is
   * correct and avoids per-subclass copy-paste.
   *
   * Why this exists: assigning `innerHTML` on a live element destroys
   * any Lit-managed reactive children and re-parses HTML even when the
   * content is fully under our control. The detached-template path
   * avoids both problems and shrinks the XSS surface (user text that
   * goes through `textContent` is unaffected by this parse).
   */
  renderMessageElement(message: ChatMessageEntity, currentUserId: string): HTMLElement | null {
    try {
      const data = this.parseContent(message);
      if (!data) return null;
      this.contentData = data;

      const wrapper = this.createAdapterWrapper();
      const contentHtml = this.renderContent(data, currentUserId);

      // Parse the rich content on a detached <template>. Its content is
      // a DocumentFragment, which we adopt into the wrapper via
      // appendChild — never via innerHTML on the wrapper itself.
      const template = globalThis.document.createElement('template');
      template.innerHTML = contentHtml;
      wrapper.appendChild(template.content.cloneNode(true));
      return wrapper;
    } catch (error) {
      console.error(`${this.constructor?.name ?? 'AbstractMessageAdapter'}.renderMessageElement failed:`, error);
      return null;
    }
  }

  /**
   * Helper for subclasses: build the standard `message-content-adapter`
   * wrapper HTMLElement with the correct classes + data attribute.
   * Subclasses append their own content into this wrapper.
   */
  protected createAdapterWrapper(): HTMLElement {
    const wrapper = document.createElement('div');
    const classes = [
      'message-content-adapter',
      `content-type-${this.contentType}`,
      ...this.getContentClasses(),
      ...(this.options.customClassNames || [])
    ];
    wrapper.className = classes.join(' ');
    wrapper.dataset.contentType = this.contentType;
    return wrapper;
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
    verbose() && console.log(`🔄 Setting up intersection observer for ${this.contentType}`);
  }

  /**
   * Set up user interaction handlers
   *
   * NOTE: We NO LONGER add per-element event listeners here.
   * Instead, use data-action attributes in your HTML and register handlers
   * with the MessageEventDelegator in ChatWidget.
   *
   * This prevents memory leaks when message elements are removed from DOM.
   * The delegator uses event bubbling from a single listener on the container.
   *
   * Example HTML: <button data-action="fullscreen">View</button>
   * Example delegator: delegator.onAction('fullscreen', handler)
   */
  protected setupInteractionHandlers(_element: HTMLElement): void {
    // NO-OP: Event delegation handles this via MessageEventDelegator
    // Subclasses should NOT add addEventListener to dynamic message elements
  }

  /**
   * Future: AI editing capabilities
   * Each adapter can define what AI can edit about its content
   */
  protected getAIEditableFields(): Record<string, string> {
    return {}; // Override in subclasses
  }

  /**
   * Future: Handle AI-driven content editing
   */
  async handleAIEdit(editInstructions: Record<string, unknown>): Promise<void> {
    if (!this.options.aiEditingEnabled) return;

    verbose() && console.log(`🤖 AI editing ${this.contentType}:`, editInstructions);
    this.hooks.onAIEdit?.(editInstructions);
  }
}