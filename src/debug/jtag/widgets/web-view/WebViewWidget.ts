/**
 * WebViewWidget - Co-browsing widget for AI context awareness
 *
 * Enables AIs to "see" what web content the user is viewing.
 * Fetches web pages via CORS proxy and extracts content for RAG context.
 *
 * Key capability: AI assistants can provide help based on the actual
 * web page content the user is looking at.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import type { WebFetchParams, WebFetchResult } from '../../commands/interface/web/fetch/shared/WebFetchTypes';

interface PageContent {
  url: string;
  title: string;
  description: string;
  textContent: string;
  links: Array<{ text: string; href: string }>;
  headings: string[];
  fetchedAt: number;
}

export class WebViewWidget extends BaseWidget {
  private currentUrl: string = '';
  private pageContent: PageContent | null = null;
  private isLoading: boolean = false;
  private error: string | null = null;

  // DOM references
  private urlInput!: HTMLInputElement;
  private contentFrame!: HTMLDivElement;
  private loadingIndicator!: HTMLDivElement;
  private errorDisplay!: HTMLDivElement;

  constructor() {
    super({
      widgetName: 'WebViewWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('WebView: Initializing co-browsing widget...');
    this.emitPositronContext();
  }

  protected async renderWidget(): Promise<void> {
    this.render();
  }

  protected async onWidgetCleanup(): Promise<void> {
    // No cleanup needed
  }

  /**
   * Render the widget UI
   */
  private render(): void {
    const container = this.shadowRoot || this;

    container.innerHTML = `
      <style>
        :host {
          display: flex;
          flex-direction: column;
          height: 100%;
          background: var(--surface-background, rgba(15, 20, 25, 0.95));
          color: var(--text-primary, #e0e0e0);
          font-family: var(--font-family, 'Inter', sans-serif);
        }

        .url-bar {
          display: flex;
          gap: 8px;
          padding: 12px;
          background: var(--surface-elevated, rgba(25, 30, 40, 0.9));
          border-bottom: 1px solid var(--border-subtle, rgba(0, 212, 255, 0.2));
        }

        .url-input {
          flex: 1;
          padding: 8px 12px;
          background: var(--input-background, rgba(0, 0, 0, 0.3));
          border: 1px solid var(--border-subtle, rgba(0, 212, 255, 0.3));
          border-radius: 6px;
          color: var(--text-primary, #e0e0e0);
          font-size: 14px;
          outline: none;
        }

        .url-input:focus {
          border-color: var(--color-primary, #00d4ff);
          box-shadow: 0 0 0 2px rgba(0, 212, 255, 0.2);
        }

        .go-button {
          padding: 8px 16px;
          background: var(--color-primary, #00d4ff);
          border: none;
          border-radius: 6px;
          color: #000;
          font-weight: 600;
          cursor: pointer;
          transition: opacity 0.2s;
        }

        .go-button:hover {
          opacity: 0.9;
        }

        .go-button:disabled {
          opacity: 0.5;
          cursor: not-allowed;
        }

        .content-area {
          flex: 1;
          overflow: auto;
          padding: 16px;
        }

        .loading {
          display: flex;
          align-items: center;
          justify-content: center;
          height: 100%;
          color: var(--text-secondary, #888);
        }

        .loading::after {
          content: '';
          width: 24px;
          height: 24px;
          margin-left: 12px;
          border: 2px solid var(--color-primary, #00d4ff);
          border-top-color: transparent;
          border-radius: 50%;
          animation: spin 1s linear infinite;
        }

        @keyframes spin {
          to { transform: rotate(360deg); }
        }

        .error {
          padding: 16px;
          background: rgba(255, 80, 80, 0.1);
          border: 1px solid rgba(255, 80, 80, 0.3);
          border-radius: 8px;
          color: #ff5050;
        }

        .page-content {
          line-height: 1.6;
        }

        .page-content h1, .page-content h2, .page-content h3 {
          color: var(--color-primary, #00d4ff);
          margin-top: 1.5em;
          margin-bottom: 0.5em;
        }

        .page-content h1 { font-size: 1.5em; }
        .page-content h2 { font-size: 1.3em; }
        .page-content h3 { font-size: 1.1em; }

        .page-content p {
          margin-bottom: 1em;
        }

        .page-content a {
          color: var(--color-primary, #00d4ff);
          text-decoration: none;
        }

        .page-content a:hover {
          text-decoration: underline;
        }

        .page-meta {
          margin-bottom: 16px;
          padding-bottom: 16px;
          border-bottom: 1px solid var(--border-subtle, rgba(0, 212, 255, 0.2));
        }

        .page-meta .title {
          font-size: 1.2em;
          font-weight: 600;
          color: var(--text-primary, #e0e0e0);
          margin-bottom: 8px;
        }

        .page-meta .description {
          color: var(--text-secondary, #888);
          font-size: 0.9em;
        }

        .empty-state {
          display: flex;
          flex-direction: column;
          align-items: center;
          justify-content: center;
          height: 100%;
          color: var(--text-secondary, #888);
          text-align: center;
        }

        .empty-state .icon {
          font-size: 48px;
          margin-bottom: 16px;
          opacity: 0.5;
        }

        .ai-context-badge {
          position: absolute;
          top: 8px;
          right: 8px;
          padding: 4px 8px;
          background: rgba(0, 212, 255, 0.2);
          border: 1px solid rgba(0, 212, 255, 0.4);
          border-radius: 4px;
          font-size: 10px;
          color: var(--color-primary, #00d4ff);
        }
      </style>

      <div class="url-bar">
        <input type="text" class="url-input" placeholder="Enter URL to browse (e.g., https://example.com)" />
        <button class="go-button">Go</button>
      </div>

      <div class="content-area">
        <div class="empty-state">
          <div class="icon">🌐</div>
          <div>Enter a URL above to browse</div>
          <div style="margin-top: 8px; font-size: 0.85em;">
            AI assistants will be able to see the page content
          </div>
        </div>
      </div>
    `;

    // Get DOM references
    this.urlInput = container.querySelector('.url-input') as HTMLInputElement;
    this.contentFrame = container.querySelector('.content-area') as HTMLDivElement;

    // Set up event listeners
    const goButton = container.querySelector('.go-button') as HTMLButtonElement;
    goButton.addEventListener('click', () => this.loadUrl());

    this.urlInput.addEventListener('keypress', (e) => {
      if (e.key === 'Enter') {
        this.loadUrl();
      }
    });
  }

  /**
   * Load a URL via CORS proxy
   */
  async loadUrl(url?: string): Promise<void> {
    const targetUrl = url || this.urlInput.value.trim();

    if (!targetUrl) {
      this.showError('Please enter a URL');
      return;
    }

    // Normalize URL
    let normalizedUrl = targetUrl;
    if (!normalizedUrl.startsWith('http://') && !normalizedUrl.startsWith('https://')) {
      normalizedUrl = 'https://' + normalizedUrl;
    }

    this.currentUrl = normalizedUrl;
    this.isLoading = true;
    this.error = null;
    this.showLoading();

    try {
      // Fetch via server-side command (bypasses CORS)
      const result = await this.executeCommand<WebFetchParams, WebFetchResult>(
        'interface/web/fetch',
        {
          url: normalizedUrl,
          format: 'text',
          maxLength: 10000  // Limit for RAG context efficiency
        }
      );

      if (!result.success) {
        throw new Error(result.error || 'Failed to fetch page');
      }

      // Extract headings from content (simple heuristic)
      const headings = this.extractHeadings(result.content || '');

      // Store the content
      this.pageContent = {
        url: result.finalUrl || normalizedUrl,
        title: result.title || this.extractDomain(normalizedUrl),
        description: '',  // Not available from text format
        textContent: result.content || '',
        links: [],  // Not extracted in text format
        headings,
        fetchedAt: Date.now()
      };

      this.renderContent();
      this.emitPositronContext();

    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      this.showError(`Failed to load page: ${errorMsg}`);
      console.error('WebView: Error loading URL:', error);
    } finally {
      this.isLoading = false;
    }
  }

  /**
   * Show loading state
   */
  private showLoading(): void {
    this.contentFrame.innerHTML = `
      <div class="loading">Loading ${this.currentUrl}...</div>
    `;
  }

  /**
   * Show error state
   */
  private showError(message: string): void {
    this.error = message;
    this.contentFrame.innerHTML = `
      <div class="error">${message}</div>
    `;
  }

  /**
   * Render the fetched content
   */
  private renderContent(): void {
    if (!this.pageContent) return;

    const { title, description, textContent, headings } = this.pageContent;

    // Truncate content for display (full content goes to AI context)
    const displayContent = textContent.length > 5000
      ? textContent.slice(0, 5000) + '...'
      : textContent;

    this.contentFrame.innerHTML = `
      <div class="ai-context-badge">AI can see this page</div>
      <div class="page-meta">
        <div class="title">${this.escapeHtml(title)}</div>
        ${description ? `<div class="description">${this.escapeHtml(description)}</div>` : ''}
      </div>
      <div class="page-content">
        ${headings.length > 0 ? `
          <h3>Page Sections</h3>
          <ul>
            ${headings.slice(0, 10).map(h => `<li>${this.escapeHtml(h)}</li>`).join('')}
          </ul>
        ` : ''}
        <h3>Content</h3>
        <div style="white-space: pre-wrap;">${this.escapeHtml(displayContent)}</div>
      </div>
    `;
  }

  /**
   * Emit Positron context for AI awareness
   */
  private emitPositronContext(): void {
    if (this.pageContent) {
      // Emit rich context with page content
      PositronWidgetState.emit(
        {
          widgetType: 'web-view',
          title: `Browsing: ${this.pageContent.title}`,
          metadata: {
            url: this.pageContent.url,
            pageTitle: this.pageContent.title,
            pageDescription: this.pageContent.description,
            // Include summarized content for AI (truncated for token efficiency)
            pageContent: this.pageContent.textContent.slice(0, 2000),
            headings: this.pageContent.headings.slice(0, 10),
            linkCount: this.pageContent.links.length,
            fetchedAt: this.pageContent.fetchedAt
          }
        },
        {
          action: 'viewing',
          target: this.pageContent.title,
          details: `User is browsing: ${this.pageContent.url}`
        }
      );
    } else {
      // Emit empty state
      PositronWidgetState.emit(
        {
          widgetType: 'web-view',
          title: 'Web Browser',
          metadata: {
            state: 'empty',
            message: 'No page loaded'
          }
        },
        { action: 'viewing', target: 'web browser (empty)' }
      );
    }
  }

  /**
   * Extract headings from text content (simple heuristic)
   */
  private extractHeadings(text: string): string[] {
    const lines = text.split('\n');
    const headings: string[] = [];

    for (const line of lines) {
      const trimmed = line.trim();
      // Heuristic: short lines (5-80 chars) that are capitalized or end with no punctuation
      // might be headings
      if (trimmed.length >= 5 && trimmed.length <= 80) {
        // Check if it looks like a heading (all caps, title case, or no ending punctuation)
        const isAllCaps = trimmed === trimmed.toUpperCase() && /[A-Z]/.test(trimmed);
        const noEndPunct = !/[.!?:;,]$/.test(trimmed);
        const startsCapital = /^[A-Z]/.test(trimmed);

        if ((isAllCaps || (startsCapital && noEndPunct)) && headings.length < 20) {
          headings.push(trimmed);
        }
      }
    }

    return headings;
  }

  /**
   * Extract domain from URL
   */
  private extractDomain(url: string): string {
    try {
      const urlObj = new URL(url);
      return urlObj.hostname;
    } catch {
      return url;
    }
  }

  /**
   * Escape HTML to prevent XSS
   */
  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}

// Register custom element
customElements.define('web-view-widget', WebViewWidget);
