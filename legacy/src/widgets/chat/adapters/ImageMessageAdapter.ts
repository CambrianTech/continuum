/**
 * Image Message Content Adapter
 *
 * Simple, efficient image renderer with lazy loading
 * No shadow DOM per row - just embedded CSS and HTML
 */

import type { ChatMessageEntity, MediaItem } from '../../../system/data/entities/ChatMessageEntity';
import { AbstractMessageAdapter } from './AbstractMessageAdapter';

// Verbose logging helper for browser
const verbose = () => typeof window !== 'undefined' && window.JTAG_VERBOSE === true;

interface ImageContentData {
  readonly images: readonly MediaItem[];  // Support multiple images
  readonly caption?: string;               // Text that wasn't a placeholder
}

export class ImageMessageAdapter extends AbstractMessageAdapter<ImageContentData> {
  constructor(options = {}, hooks = {}) {
    super('image', {
      enableIntersectionObserver: true,
      lazyLoadContent: true,
      enableInteractions: true,
      aiEditingEnabled: true, // AI can generate alt text, captions
      ...options
    }, hooks);
  }

  /**
   * Parse image data from MediaItem array
   * Supports multiple images with [Image #N] placeholders
   */
  parseContent(message: ChatMessageEntity): ImageContentData | null {
    const media = message.content?.media;
    const text = message.content?.text || '';

    // Filter for image media items
    const images = media?.filter(m => m.type === 'image') ?? [];
    if (images.length === 0) return null;

    // Remove [Image #N] placeholders from text to get caption
    const caption = text.replace(/\[Image #\d+\]/g, '').trim() || undefined;

    return {
      images,
      caption
    };
  }

  /**
   * Render responsive images with loading states
   * Supports multiple images from MediaItem array
   */
  renderContent(data: ImageContentData, _currentUserId: string): string {
    const { images, caption } = data;

    // Render each image
    const imagesHtml = images.map((mediaItem, index) => {
      const imageId = `img-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      const url = mediaItem.url ?? (mediaItem.base64 ? `data:${mediaItem.mimeType ?? 'image/png'};base64,${mediaItem.base64}` : '');
      const altText = mediaItem.alt ?? mediaItem.description ?? `Image ${index + 1}`;
      const filename = mediaItem.filename ?? `image-${index + 1}`;

      return `
        <div class="image-container" data-image-id="${imageId}" data-media-id="${mediaItem.id ?? ''}">
          <div class="image-loading-placeholder">
            <div class="loading-spinner"></div>
            <span class="loading-text">Loading image...</span>
          </div>
          <img
            src="${url}"
            alt="${altText}"
            class="message-image"
            loading="lazy"
            data-loaded="false"
            data-width="${mediaItem.width ?? ''}"
            data-height="${mediaItem.height ?? ''}"
            style="display: block; max-width: 100%; height: auto;"
          />
          <div class="image-error" style="display: none;">
            <span class="error-icon">🖼️</span>
            <span class="error-text">Image failed to load</span>
            <button class="retry-button" data-action="image-retry" data-url="${url}">Retry</button>
          </div>
          <div class="image-actions">
            <button class="action-button" data-action="image-fullscreen" title="View fullscreen">🔍</button>
            <button class="action-button" data-action="image-download" data-url="${url}" data-filename="${filename}" title="Download">⬇️</button>
            <button class="action-button" data-action="image-ai-describe" title="AI describe image">🤖</button>
          </div>
        </div>
      `;
    }).join('');

    return `
      <div class="image-message-content">
        <div class="images-grid ${images.length > 1 ? 'multiple-images' : 'single-image'}">
          ${imagesHtml}
        </div>
        ${caption ? `<div class="image-caption">${caption}</div>` : ''}
      </div>
    `;
  }

  /**
   * DOM-returning render path (see issue #1100). Builds the entire
   * image-content structure via DOM APIs instead of HTML strings.
   *
   * Why this is a meaningful security improvement (not just refactor):
   * the string path interpolated user-controllable values directly into
   * HTML attribute positions — `src="${url}"`, `alt="${altText}"`,
   * `data-filename="${filename}"`, and especially `${caption}` in
   * element-content position. Any one of those is an XSS opportunity
   * if the source data isn't perfectly escaped. Here every dynamic
   * value is set via property assignment (`img.src = url`, `img.alt =`)
   * or `.textContent` (caption), where the browser cannot reinterpret
   * the value as markup. Class names, structure, and CSS hooks are
   * preserved verbatim so `handleContentLoading()` and the event
   * delegator still find their selectors.
   */
  override renderMessageElement(message: ChatMessageEntity, _currentUserId: string): HTMLElement | null {
    try {
      const data = this.parseContent(message);
      if (!data) return null;
      this.contentData = data;

      const wrapper = this.createAdapterWrapper();

      const content = document.createElement('div');
      content.className = 'image-message-content';
      wrapper.appendChild(content);

      const grid = document.createElement('div');
      grid.className = `images-grid ${data.images.length > 1 ? 'multiple-images' : 'single-image'}`;
      content.appendChild(grid);

      data.images.forEach((mediaItem, index) => {
        grid.appendChild(this.buildImageContainer(mediaItem, index));
      });

      if (data.caption) {
        const captionEl = document.createElement('div');
        captionEl.className = 'image-caption';
        // textContent — caption originates from message.content.text and
        // must not be interpreted as markup.
        captionEl.textContent = data.caption;
        content.appendChild(captionEl);
      }

      return wrapper;
    } catch (error) {
      console.error('ImageMessageAdapter.renderMessageElement failed:', error);
      return null;
    }
  }

  /**
   * Build a single .image-container element with its loading placeholder,
   * <img>, error overlay, and action buttons. Structure mirrors the
   * string-based renderContent exactly so handleContentLoading() and
   * the event-delegated action buttons keep working.
   */
  private buildImageContainer(mediaItem: MediaItem, index: number): HTMLElement {
    const imageId = `img-${Date.now()}-${Math.random().toString(36).slice(2, 11)}`;
    const url = mediaItem.url ?? (mediaItem.base64 ? `data:${mediaItem.mimeType ?? 'image/png'};base64,${mediaItem.base64}` : '');
    const altText = mediaItem.alt ?? mediaItem.description ?? `Image ${index + 1}`;
    const filename = mediaItem.filename ?? `image-${index + 1}`;

    const container = document.createElement('div');
    container.className = 'image-container';
    container.dataset.imageId = imageId;
    container.dataset.mediaId = mediaItem.id ?? '';

    // Loading placeholder
    const placeholder = document.createElement('div');
    placeholder.className = 'image-loading-placeholder';
    const spinner = document.createElement('div');
    spinner.className = 'loading-spinner';
    const loadingText = document.createElement('span');
    loadingText.className = 'loading-text';
    loadingText.textContent = 'Loading image...';
    placeholder.appendChild(spinner);
    placeholder.appendChild(loadingText);
    container.appendChild(placeholder);

    // Image — property assignment for url/alt, never attribute interpolation.
    const img = document.createElement('img');
    img.src = url;
    img.alt = altText;
    img.className = 'message-image';
    img.loading = 'lazy';
    img.dataset.loaded = 'false';
    if (mediaItem.width !== undefined) img.dataset.width = String(mediaItem.width);
    if (mediaItem.height !== undefined) img.dataset.height = String(mediaItem.height);
    img.style.display = 'block';
    img.style.maxWidth = '100%';
    img.style.height = 'auto';
    container.appendChild(img);

    // Error overlay
    const errorDiv = document.createElement('div');
    errorDiv.className = 'image-error';
    errorDiv.style.display = 'none';
    const errorIcon = document.createElement('span');
    errorIcon.className = 'error-icon';
    errorIcon.textContent = '🖼️';
    const errorText = document.createElement('span');
    errorText.className = 'error-text';
    errorText.textContent = 'Image failed to load';
    const retryBtn = document.createElement('button');
    retryBtn.className = 'retry-button';
    retryBtn.dataset.action = 'image-retry';
    retryBtn.dataset.url = url;
    retryBtn.textContent = 'Retry';
    errorDiv.appendChild(errorIcon);
    errorDiv.appendChild(errorText);
    errorDiv.appendChild(retryBtn);
    container.appendChild(errorDiv);

    // Action buttons
    const actions = document.createElement('div');
    actions.className = 'image-actions';
    actions.appendChild(this.buildActionButton('image-fullscreen', '🔍', 'View fullscreen'));
    const downloadBtn = this.buildActionButton('image-download', '⬇️', 'Download');
    downloadBtn.dataset.url = url;
    downloadBtn.dataset.filename = filename;
    actions.appendChild(downloadBtn);
    actions.appendChild(this.buildActionButton('image-ai-describe', '🤖', 'AI describe image'));
    container.appendChild(actions);

    return container;
  }

  private buildActionButton(action: string, label: string, title: string): HTMLButtonElement {
    const btn = document.createElement('button');
    btn.className = 'action-button';
    btn.dataset.action = action;
    btn.title = title;
    // aria-label complements the title — title is unreliable for SR.
    btn.setAttribute('aria-label', title);
    btn.textContent = label;
    return btn;
  }

  /**
   * Handle image loading with proper error states and lazy loading
   */
  async handleContentLoading(element: HTMLElement): Promise<void> {
    const img = element.querySelector('.message-image') as HTMLImageElement;
    const placeholder = element.querySelector('.image-loading-placeholder') as HTMLElement;
    const errorDiv = element.querySelector('.image-error') as HTMLElement;

    if (!img) return;

    return new Promise((resolve) => {
      const onLoad = () => {
        img.style.display = 'block';
        img.dataset.loaded = 'true';
        placeholder.style.display = 'none';
        errorDiv.style.display = 'none';
        resolve();
      };

      const onError = () => {
        placeholder.style.display = 'none';
        errorDiv.style.display = 'block';
        resolve(); // Still resolve to not block other content
      };

      // Set up event listeners
      img.addEventListener('load', onLoad, { once: true });
      img.addEventListener('error', onError, { once: true });

      // If image is already loaded (cached), trigger immediately
      if (img.complete && img.naturalWidth > 0) {
        onLoad();
      }
    });
  }

  /**
   * CSS classes specific to image content
   */
  getContentClasses(): string[] {
    return ['image-content', 'media-content', 'interactive-content'];
  }

  /**
   * CSS for image message content (injected once into chat widget shadow DOM)
   */
  getCSS(): string {
    return `
      /* Image Message Adapter Styles */
      .content-type-image {
        max-width: 100%;
        margin: 8px 0;
      }

      .image-message-content {
        border-radius: 8px;
        overflow: hidden;
      }

      /* Grid layout for multiple images */
      .images-grid {
        display: grid;
        gap: 8px;
        padding: 8px;
      }

      .images-grid.single-image {
        grid-template-columns: 1fr;
      }

      .images-grid.multiple-images {
        grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
      }

      .image-container {
        position: relative;
        max-width: 400px;
        /* No max-height — clipped tall images (verified 2026-04-21
         * with the cat photo: vertical aspect was hitting the 300px
         * cap and getting cut off at the bottom). Aspect ratio comes
         * from the image itself; display:inline-block makes the
         * container size to the rendered image so the bubble doesn't
         * leave a wide background-colored gutter to the right. */
        display: inline-block;
        border-radius: 8px;
        overflow: hidden;
        background: var(--loading-bg, #e0e0e0);
      }

      .images-grid.multiple-images .image-container {
        max-width: 100%;
      }

      .image-loading-placeholder {
        display: flex;
        align-items: center;
        justify-content: center;
        min-height: 100px;
        background: var(--loading-bg, #e0e0e0);
        color: var(--loading-text, #666);
      }

      .loading-spinner {
        width: 16px;
        height: 16px;
        border: 2px solid transparent;
        border-top: 2px solid currentColor;
        border-radius: 50%;
        animation: spin 1s linear infinite;
        margin-right: 8px;
      }

      @keyframes spin {
        to { transform: rotate(360deg); }
      }

      .message-image {
        width: 100%;
        height: auto;
        display: block;
        transition: opacity 0.2s ease;
      }

      .image-error {
        display: flex;
        flex-direction: column;
        align-items: center;
        padding: 20px;
        background: var(--error-bg, #fff3f3);
        color: var(--error-text, #d73a49);
      }

      .retry-button {
        margin-top: 8px;
        padding: 4px 12px;
        background: var(--button-bg, #007acc);
        color: var(--button-text, white);
        border: none;
        border-radius: 4px;
        cursor: pointer;
        font-size: 12px;
      }

      .image-caption {
        padding: 8px 12px;
        font-size: 14px;
        color: var(--caption-text, #666);
        background: var(--caption-bg, rgba(0,0,0,0.05));
      }

      .image-actions {
        display: flex;
        gap: 4px;
        padding: 8px;
        background: var(--actions-bg, rgba(0,0,0,0.05));
        justify-content: flex-end;
      }

      .action-button {
        background: none;
        border: none;
        cursor: pointer;
        padding: 4px;
        border-radius: 4px;
        opacity: 0.7;
        transition: opacity 0.2s ease;
      }

      .action-button:hover {
        opacity: 1;
        background: var(--button-hover-bg, rgba(0,0,0,0.1));
      }
    `;
  }

  // NOTE: setupInteractionHandlers removed - now uses event delegation
  // Action handlers are static and called by MessageEventDelegator in ChatWidget

  /**
   * Static action handlers for event delegation
   * These are called by MessageEventDelegator, not per-element listeners
   */

  /**
   * Open image in fullscreen mode
   */
  static handleFullscreen(target: HTMLElement): void {
    const container = target.closest('.image-container');
    const img = container?.querySelector('.message-image') as HTMLImageElement;
    if (!img) return;

    verbose() && console.log('🖼️ Opening fullscreen for:', img.src);
    // Future: Implement fullscreen overlay
  }

  /**
   * Download image
   */
  static handleDownload(target: HTMLElement): void {
    if (typeof document === 'undefined') return;

    const url = target.dataset.url;
    const filename = target.dataset.filename;
    if (!url) return;

    const a = document.createElement('a');
    a.href = url;
    a.download = filename ?? url.split('/').pop() ?? 'image';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);

    verbose() && console.log('⬇️ Downloaded:', filename);
  }

  /**
   * Request AI-generated description
   */
  static handleAIDescribe(target: HTMLElement): void {
    const container = target.closest('.image-container');
    const img = container?.querySelector('.message-image') as HTMLImageElement;
    if (!img) return;

    verbose() && console.log('🤖 Requesting AI description for:', img.src);
    // Future: Call AI service for image description
  }

  /**
   * Retry loading a failed image
   */
  static handleRetry(target: HTMLElement): void {
    const url = target.dataset.url;
    const container = target.closest('.image-container');
    if (!url || !container) return;

    const img = container.querySelector('.message-image') as HTMLImageElement;
    const placeholder = container.querySelector('.image-loading-placeholder') as HTMLElement;
    const errorDiv = container.querySelector('.image-error') as HTMLElement;

    if (img && placeholder && errorDiv) {
      placeholder.style.display = 'flex';
      errorDiv.style.display = 'none';
      img.src = ''; // Force reload
      img.src = url;
    }
  }

  /**
   * AI-editable fields for this content type
   */
  protected getAIEditableFields(): Record<string, string> {
    return {
      altText: 'string',
      caption: 'string',
      description: 'string'
    };
  }

  /**
   * Handle AI editing of image content
   */
  async handleAIEdit(editInstructions: Record<string, unknown>): Promise<void> {
    verbose() && console.log('🤖 AI editing image content:', editInstructions);

    // Future: AI can:
    // - Generate better alt text
    // - Create captions
    // - Suggest image improvements
    // - Auto-crop/enhance images

    if (editInstructions.generateAltText) {
      // const newAltText = await aiService.generateAltText(this.contentData?.url);
      // Update the image alt text
    }

    if (editInstructions.generateCaption) {
      // const newCaption = await aiService.generateCaption(this.contentData?.url);
      // Update the caption
    }

    await super.handleAIEdit(editInstructions);
  }
}