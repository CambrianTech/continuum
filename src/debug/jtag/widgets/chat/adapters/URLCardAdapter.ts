/**
 * URL Card Message Content Adapter
 *
 * Handles link previews with rich metadata, favicons,
 * and future AI capabilities (content summarization, link verification)
 */

import type { ChatMessageEntity } from '../../../system/data/entities/ChatMessageEntity';
import { AbstractMessageAdapter } from './AbstractMessageAdapter';

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
   * Render rich URL card with metadata
   */
  renderContent(data: URLCardData, currentUserId: string): string {
    const { url, title, description, siteName, favicon, domain, isSecure, originalText } = data;
    const cardId = `url-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

    // Extract any text that isn't the URL
    const additionalText = originalText.replace(url, '').trim();

    return `
      <div class="url-card-content">
        ${additionalText ? `<div class="url-message-text">${additionalText}</div>` : ''}

        <div class="url-card" data-card-id="${cardId}" data-url="${url}">
          <div class="url-card-loading" style="display: block;">
            <div class="loading-spinner"></div>
            <span class="loading-text">Loading preview...</span>
          </div>

          <div class="url-card-content-area" style="display: none;">
            <div class="url-card-header">
              <img src="${favicon}" alt="${domain} favicon" class="site-favicon" loading="lazy" />
              <div class="site-info">
                <span class="site-name">${siteName}</span>
                <span class="url-domain ${isSecure ? 'secure' : 'insecure'}">
                  ${isSecure ? '🔒' : '🔓'} ${domain}
                </span>
              </div>
              <div class="card-actions">
                <button class="action-button ai-summarize" title="AI summarize">🤖</button>
                <button class="action-button external-link" title="Open in new tab">↗️</button>
              </div>
            </div>

            <div class="url-card-body">
              <h3 class="url-title">${title}</h3>
              <p class="url-description">${description}</p>
              <div class="url-metadata">
                <span class="url-full" title="${url}">${url}</span>
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
              <button class="retry-preview" data-url="${url}">Retry</button>
            </div>
            <div class="fallback-link">
              <a href="${url}" target="_blank" rel="noopener noreferrer" class="external-link-fallback">
                ${url}
              </a>
            </div>
          </div>
        </div>
      </div>
    `;
  }

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
  private generateMockMetadata(url: string): any {
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
  private updateCardWithMetadata(element: HTMLElement, metadata: any): void {
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

  /**
   * Enhanced interaction handlers for URL cards
   */
  protected setupInteractionHandlers(element: HTMLElement): void {
    super.setupInteractionHandlers(element);

    // External link opening
    const externalBtn = element.querySelector('.external-link');
    const fallbackLink = element.querySelector('.external-link-fallback');

    [externalBtn, fallbackLink].forEach(btn => {
      btn?.addEventListener('click', (e) => {
        e.stopPropagation();
        const url = element.querySelector('.url-card')?.getAttribute('data-url');
        if (url) this.openExternalLink(url);
      });
    });

    // AI summarization
    const aiBtn = element.querySelector('.ai-summarize');
    aiBtn?.addEventListener('click', (e) => {
      e.stopPropagation();
      this.requestAISummary(element);
    });

    // Retry preview
    const retryBtn = element.querySelector('.retry-preview');
    retryBtn?.addEventListener('click', (e) => {
      e.stopPropagation();
      this.retryPreview(element);
    });

    // Card click (full card is clickable)
    const card = element.querySelector('.url-card');
    card?.addEventListener('click', (e) => {
      // Don't trigger if clicking on buttons
      if ((e.target as HTMLElement).tagName === 'BUTTON') return;

      const url = card.getAttribute('data-url');
      if (url) this.openExternalLink(url);
    });
  }

  /**
   * Open URL in new tab safely
   */
  private openExternalLink(url: string): void {
    window.open(url, '_blank', 'noopener,noreferrer');
    this.hooks.onUserInteraction?.('open_link', { url });
  }

  /**
   * Request AI summarization of the linked content
   */
  private async requestAISummary(element: HTMLElement): Promise<void> {
    const url = element.querySelector('.url-card')?.getAttribute('data-url');
    if (!url) return;

    console.log('🤖 Requesting AI summary for:', url);

    // Future: AI content summarization
    // const summary = await aiService.summarizeURL(url);

    this.hooks.onUserInteraction?.('ai_summarize', {
      url,
      action: 'summarize_content'
    });
  }

  /**
   * Retry preview loading
   */
  private async retryPreview(element: HTMLElement): Promise<void> {
    const loadingDiv = element.querySelector('.url-card-loading') as HTMLElement;
    const contentDiv = element.querySelector('.url-card-content-area') as HTMLElement;
    const errorDiv = element.querySelector('.url-card-error') as HTMLElement;

    // Reset states
    errorDiv.style.display = 'none';
    loadingDiv.style.display = 'block';
    contentDiv.style.display = 'none';

    // Retry loading
    await this.handleContentLoading(element);
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
  protected getAIEditableFields(): Record<string, any> {
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
  async handleAIEdit(editInstructions: any): Promise<void> {
    console.log('🤖 AI editing URL card:', editInstructions);

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