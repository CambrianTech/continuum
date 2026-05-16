/**
 * URL Card Message Content Adapter
 *
 * Handles link previews with rich metadata, favicons,
 * and future AI capabilities (content summarization, link verification)
 */

import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import { AbstractMessageAdapter } from './AbstractMessageAdapter';

// Verbose logging helper for browser
import type { JTAGWindowProperties } from '../../../system/core/types/GlobalAugmentations';

const verbose = () => typeof window !== 'undefined' && (window as Window & JTAGWindowProperties).JTAG_VERBOSE === true;

interface URLCardMetadata {
  readonly title: string;
  readonly description: string;
  readonly siteName: string;
  readonly imageUrl?: string;
}

interface URLCardData {
  readonly url: string;
  readonly title?: string;
  readonly description?: string;
  readonly siteName?: string;
  readonly favicon?: string;
  readonly imageUrl?: string;
  readonly domain: string;
  readonly isSecure: boolean;
  readonly originalText: string;
}

export class URLCardAdapter extends AbstractMessageAdapter<URLCardData> {
  constructor(options = {}, hooks = {}) {
    super('url_card', {
      enableIntersectionObserver: true,
      lazyLoadContent: true,
      enableInteractions: true,
      aiEditingEnabled: true, // AI can summarize, verify links
      ...options
    }, hooks);
  }

  /**
   * Parse URL and extract metadata from message text
   */
  parseContent(message: ChatMessageEntity): URLCardData | null {
    const text = message.content?.text;
    if (!text) return null;

    // Extract URL from text
    const urlMatch = text.match(/(https?:\/\/[^\s]+)/i);
    if (!urlMatch) return null;

    const url = urlMatch[1];
    const urlObj = new URL(url);
    const domain = urlObj.hostname;
    const isSecure = urlObj.protocol === 'https:';

    return {
      url,
      domain,
      isSecure,
      originalText: text,
      title: `Link to ${domain}`, // AI will improve this
      description: 'Loading preview...', // Will be fetched/AI-generated
      siteName: domain,
      favicon: `https://www.google.com/s2/favicons?domain=${domain}&sz=32`
    };
  }

  /**
   * Render rich URL card with metadata.
   *
   * **XSS hardening (#1159 — closes the metadata-XSS surface PR-1
   * deferred):** every interpolation is now passed through `escapeHtml`
   * before landing in the HTML template. Three classes of input feed
   * the template:
   *   1. Raw user text (`originalText`, `additionalText`) — directly
   *      from chat content, fully attacker-controlled.
   *   2. Parsed URL fields (`url`, `domain`, `siteName` initial value)
   *      — parsed via `new URL()` so the hostname is structurally
   *      safe, but `url` itself is the raw input string and may
   *      contain quotes, angle brackets, or a `javascript:` scheme.
   *   3. Async metadata (`title`, `description`, `siteName` post-fetch
   *      via `updateCardWithMetadata`) — fetched from a remote URL,
   *      attacker-controlled in the worst case.
   *
   * The `href="${url}"` slot additionally goes through `safeHref` to
   * neutralize `javascript:` / `data:` / `vbscript:` URLs (these
   * become `#` so a click does nothing instead of executing script in
   * the page's origin).
   */
  renderContent(data: URLCardData, currentUserId: string): string {
    const { url, title, description, siteName, favicon, domain, isSecure, originalText } = data;
    const cardId = `url-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

    // Extract any text that isn't the URL
    const additionalText = originalText.replace(url, '').trim();

    const safeAdditionalText = this.escapeHtml(additionalText);
    const safeUrlAttr = this.escapeHtml(url);
    const safeFavicon = this.escapeHtml(favicon ?? '');
    const safeDomain = this.escapeHtml(domain);
    const safeSiteName = this.escapeHtml(siteName ?? domain);
    const safeTitle = this.escapeHtml(title ?? '');
    const safeDescription = this.escapeHtml(description ?? '');
    const safeHrefValue = this.escapeHtml(this.safeHref(url));

    return `
      <div class="url-card-content">
        ${additionalText ? `<div class="url-message-text">${safeAdditionalText}</div>` : ''}

        <div class="url-card" data-card-id="${cardId}" data-url="${safeUrlAttr}" data-action="url-card-click">
          <div class="url-card-loading" style="display: block;">
            <div class="loading-spinner"></div>
            <span class="loading-text">Loading preview...</span>
          </div>

          <div class="url-card-content-area" style="display: none;">
            <div class="url-card-header">
              <img src="${safeFavicon}" alt="${safeDomain} favicon" class="site-favicon" loading="lazy" />
              <div class="site-info">
                <span class="site-name">${safeSiteName}</span>
                <span class="url-domain ${isSecure ? 'secure' : 'insecure'}">
                  ${isSecure ? '🔒' : '🔓'} ${safeDomain}
                </span>
              </div>
              <div class="card-actions">
                <button class="action-button" data-action="url-ai-summarize" title="AI summarize">🤖</button>
                <button class="action-button" data-action="url-open-external" title="Open in new tab">↗️</button>
              </div>
            </div>

            <div class="url-card-body">
              <h3 class="url-title">${safeTitle}</h3>
              <p class="url-description">${safeDescription}</p>
              <div class="url-metadata">
                <span class="url-full" title="${safeUrlAttr}">${safeUrlAttr}</span>
              </div>
            </div>

            <div class="url-card-image" style="display: none;">
              <img src="" alt="Preview image" class="preview-image" loading="lazy" />
            </div>
          </div>

          <div class="url-card-error" style="display: none;">
            <div class="error-content">
              <span class="error-icon">🔗</span>
              <span class="error-text">Preview unavailable</span>
              <button class="retry-preview" data-action="url-retry-preview" data-url="${safeUrlAttr}">Retry</button>
            </div>
            <div class="fallback-link">
              <a href="${safeHrefValue}" target="_blank" rel="noopener noreferrer" class="external-link-fallback">
                ${safeUrlAttr}
              </a>
            </div>
          </div>
        </div>
      </div>
    `;
  }

  /**
   * HTML-escape the 5 dangerous characters. Same shape as
   * TextMessageAdapter.escapeHtml — the canonical pattern in this
   * codebase. Safe in both text-content and double-quoted-attribute
   * contexts because it escapes both `"` and `'`.
   *
   * KEPT after the #1158 base-default lift (#1189) because URLCardAdapter's
   * `renderContent` still interpolates url/title/description/siteName as
   * raw strings into HTML — the XSS hardening from #1159 (PR #1250) lives
   * in those interpolations and depends on this method.
   */
  private escapeHtml(unsafe: string): string {
    return unsafe
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#039;');
  }

  /**
   * Neutralize dangerous URL schemes so `<a href="${safeHref(url)}">`
   * cannot execute script. Whitelist approach: keep http/https/mailto/
   * tel/ftp/sftp + protocol-relative + same-document fragments;
   * otherwise return `#` (renders as a no-op click).
   *
   * Why a whitelist not a blacklist: a blacklist of `javascript:` /
   * `data:` / `vbscript:` misses `\tjavascript:` (control-character
   * smuggling), `JaVaScRiPt:` case mixing, `&NewLine;javascript:`
   * (HTML-entity smuggling once the attribute is decoded), and any
   * future scheme that turns out to be code-executing. Whitelist of
   * known-safe schemes is the only audit-once approach.
   */
  private safeHref(url: string): string {
    if (typeof url !== 'string' || url.length === 0) return '#';
    const trimmed = url.trim();
    if (trimmed.length === 0) return '#';
    // Same-document fragment + protocol-relative URLs — both safe.
    if (trimmed.startsWith('#') || trimmed.startsWith('//')) return trimmed;
    // Schemed URL — only allow the audit-once safe set. Match scheme
    // case-insensitively because the URL spec is case-insensitive.
    const schemeMatch = trimmed.match(/^([a-z][a-z0-9+.\-]*):/i);
    if (!schemeMatch) {
      // No scheme — relative URL. Safe (cannot escape the document
      // origin without a scheme).
      return trimmed;
    }
    const scheme = schemeMatch[1].toLowerCase();
    const safeSchemes = new Set(['http', 'https', 'mailto', 'tel', 'ftp', 'sftp']);
    return safeSchemes.has(scheme) ? trimmed : '#';
  }

  // renderMessageElement: inherits the DRY base default (#1158/#1189).
  // The string `renderContent` already does the
  // template.innerHTML → cloneNode(true) DocumentFragment trick that the
  // base default expects, so the inherited path produces identical DOM
  // output. The escapeHtml + safeHref methods above stay LOCAL because
  // they're only used by this adapter's renderContent interpolation
  // hardening (#1159 PR #1250), not by the base default.

  /**
   * Handle URL metadata fetching and card population
   */
  async handleContentLoading(element: HTMLElement): Promise<void> {
    const card = element.querySelector('.url-card') as HTMLElement;
    const loadingDiv = element.querySelector('.url-card-loading') as HTMLElement;
    const contentDiv = element.querySelector('.url-card-content-area') as HTMLElement;
    const errorDiv = element.querySelector('.url-card-error') as HTMLElement;

    if (!card) return;

    const url = card.dataset.url;
    if (!url) return;

    try {
      // Simulate fetching metadata (future: real metadata service)
      await this.fetchMetadata(url, element);

      loadingDiv.style.display = 'none';
      contentDiv.style.display = 'block';
    } catch (error) {
      console.error('Failed to load URL metadata:', error);
      loadingDiv.style.display = 'none';
      errorDiv.style.display = 'block';
    }
  }

  /**
   * Fetch URL metadata (future: real implementation)
   */
  private async fetchMetadata(url: string, element: HTMLElement): Promise<void> {
    // Simulate API delay
    await new Promise(resolve => setTimeout(resolve, 1000 + Math.random() * 2000));

    // Future: Real metadata fetching
    // const metadata = await metadataService.fetch(url);

    // For demo, simulate some rich data
    const mockMetadata = this.generateMockMetadata(url);
    this.updateCardWithMetadata(element, mockMetadata);
  }

  /**
   * Generate mock metadata for demo purposes
   */
  private generateMockMetadata(url: string): URLCardMetadata {
    const domain = new URL(url).hostname;
    return {
      title: `Interesting content from ${domain}`,
      description: `Check out this fascinating content that was shared. This is a preview of what you'll find when you visit the link.`,
      siteName: domain,
      imageUrl: Math.random() > 0.5 ? 'https://picsum.photos/400/200' : undefined
    };
  }

  /**
   * Update card with fetched metadata
   */
  private updateCardWithMetadata(element: HTMLElement, metadata: URLCardMetadata): void {
    const titleEl = element.querySelector('.url-title');
    const descEl = element.querySelector('.url-description');
    const siteNameEl = element.querySelector('.site-name');
    const imageContainer = element.querySelector('.url-card-image') as HTMLElement;
    const previewImg = element.querySelector('.preview-image') as HTMLImageElement;

    if (titleEl) titleEl.textContent = metadata.title;
    if (descEl) descEl.textContent = metadata.description;
    if (siteNameEl) siteNameEl.textContent = metadata.siteName;

    if (metadata.imageUrl && previewImg) {
      previewImg.src = metadata.imageUrl;
      imageContainer.style.display = 'block';
    }
  }

  /**
   * CSS classes specific to URL card content
   */
  getContentClasses(): string[] {
    return ['url-card-content', 'interactive-content', 'rich-content'];
  }

  // NOTE: setupInteractionHandlers removed - now uses event delegation
  // Action handlers are static and called by MessageEventDelegator in ChatWidget

  /**
   * Static action handlers for event delegation
   * These are called by MessageEventDelegator, not per-element listeners
   */

  /**
   * Handle card click - open URL in new tab
   */
  static handleCardClick(target: HTMLElement, event: Event): void {
    // Don't trigger if clicking on buttons
    if ((event.target as HTMLElement).tagName === 'BUTTON') return;

    const card = target.closest('.url-card') as HTMLElement;
    const url = card?.dataset.url;
    if (url) {
      URLCardAdapter.openExternalLink(url);
    }
  }

  /**
   * Handle external link button click
   */
  static handleOpenExternal(target: HTMLElement): void {
    const card = target.closest('.url-card') as HTMLElement;
    const url = card?.dataset.url;
    if (url) {
      URLCardAdapter.openExternalLink(url);
    }
  }

  /**
   * Request AI summarization of the linked content
   */
  static handleAISummarize(target: HTMLElement): void {
    const card = target.closest('.url-card') as HTMLElement;
    const url = card?.dataset.url;
    if (!url) return;

    verbose() && console.log('🤖 Requesting AI summary for:', url);
    // Future: AI content summarization
  }

  /**
   * Retry preview loading
   */
  static handleRetryPreview(target: HTMLElement): void {
    const card = target.closest('.url-card') as HTMLElement;
    if (!card) return;

    const loadingDiv = card.querySelector('.url-card-loading') as HTMLElement;
    const contentDiv = card.querySelector('.url-card-content-area') as HTMLElement;
    const errorDiv = card.querySelector('.url-card-error') as HTMLElement;

    if (loadingDiv && contentDiv && errorDiv) {
      // Reset states to loading
      errorDiv.style.display = 'none';
      loadingDiv.style.display = 'block';
      contentDiv.style.display = 'none';

      // Note: Actual retry would need adapter instance or separate fetch
      verbose() && console.log('🔄 Retrying preview for:', card.dataset.url);
    }
  }

  /**
   * Open URL in new tab safely
   */
  private static openExternalLink(url: string): void {
    if (typeof window !== 'undefined') {
      window.open(url, '_blank', 'noopener,noreferrer');
    }
    verbose() && console.log('🔗 Opening external link:', url);
  }

  /**
   * CSS styles for URL cards
   */
  getCSS(): string {
    return `
      .content-type-url_card {
        border: 1px solid #e1e5e9;
        border-radius: 8px;
        overflow: hidden;
        margin: 8px 0;
        background: #ffffff;
      }
      .url-card-header {
        display: flex;
        align-items: center;
        padding: 12px;
        background: #f8f9fa;
        border-bottom: 1px solid #e1e5e9;
      }
      .url-card-favicon {
        width: 16px;
        height: 16px;
        margin-right: 8px;
        border-radius: 2px;
      }
      .url-card-domain {
        font-size: 12px;
        color: #6c757d;
        font-weight: 500;
      }
      .url-card-body {
        padding: 12px;
      }
      .url-card-title {
        font-weight: 600;
        margin-bottom: 4px;
        color: #1a1a1a;
      }
      .url-card-description {
        color: #666;
        font-size: 14px;
        line-height: 1.4;
        margin-bottom: 8px;
      }
      .url-card-image {
        width: 100%;
        max-height: 200px;
        object-fit: cover;
        border-radius: 4px;
      }
      .url-card-footer {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 8px 12px;
        background: #f8f9fa;
        border-top: 1px solid #e1e5e9;
      }
      .url-card-link {
        color: #007bff;
        text-decoration: none;
        font-size: 12px;
        font-weight: 500;
      }
      .url-card-link:hover {
        text-decoration: underline;
      }
      .url-card-loading, .url-card-error {
        padding: 16px;
        text-align: center;
        color: #6c757d;
      }
    `;
  }

  /**
   * AI-editable fields for URL cards
   */
  protected getAIEditableFields(): Record<string, string> {
    return {
      title: 'string',
      description: 'string',
      summary: 'string',
      tags: 'array',
      relevanceScore: 'number'
    };
  }

  /**
   * Handle AI editing of URL card content
   */
  async handleAIEdit(editInstructions: Record<string, unknown>): Promise<void> {
    verbose() && console.log('🤖 AI editing URL card:', editInstructions);

    // Future: AI can:
    // - Generate better titles
    // - Create summaries
    // - Verify link safety
    // - Extract key information
    // - Add relevance scoring

    if (editInstructions.improveTitle) {
      // const betterTitle = await aiService.improveTitle(this.contentData?.url);
    }

    if (editInstructions.generateSummary) {
      // const summary = await aiService.summarizeContent(this.contentData?.url);
    }

    super.handleAIEdit(editInstructions);
  }
}