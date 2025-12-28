/**
 * WebViewWidget - Co-browsing web content with AI context
 *
 * Displays web content and emits context to PositronWidgetState
 * so AI assistants can "see" what the user is viewing.
 *
 * Structure:
 * - web-view-widget.html - Template
 * - web-view-widget.scss - Styles (compiled to .css)
 * - WebViewWidget.ts - Logic
 */

import { BaseWidget } from '../shared/BaseWidget';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';
import type { WebFetchParams, WebFetchResult } from '@commands/interface/web/fetch/shared/WebFetchTypes';

export class WebViewWidget extends BaseWidget {
  private currentUrl: string = '';

  constructor() {
    super({
      widgetName: 'WebViewWidget',
      template: 'web-view-widget.html',
      styles: 'web-view-widget.css',
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('🌐 WebViewWidget: Initializing...');
    this.emitPositronContext();
  }

  /**
   * Override path resolution - directory is 'web-view' not 'webview'
   */
  protected resolveResourcePath(filename: string): string {
    return `widgets/web-view/public/${filename}`;
  }

  private emitPositronContext(): void {
    PositronWidgetState.emit(
      {
        widgetType: 'browser',
        title: 'Web Browser',
        metadata: {
          url: this.currentUrl || 'No page loaded'
        }
      },
      { action: 'viewing', target: this.currentUrl || 'browser' }
    );
  }

  protected async renderWidget(): Promise<void> {
    // Inject loaded template and styles into shadow DOM
    if (this.shadowRoot && (this.templateHTML || this.templateCSS)) {
      const styleTag = this.templateCSS ? `<style>${this.templateCSS}</style>` : '';
      this.shadowRoot.innerHTML = styleTag + (this.templateHTML || '');
    }

    // Set up event listeners after DOM is ready
    this.setupEventListeners();
  }

  private setupEventListeners(): void {
    const urlInput = this.shadowRoot?.querySelector('#urlInput') as HTMLInputElement;
    const goButton = this.shadowRoot?.querySelector('#goButton');

    goButton?.addEventListener('click', () => {
      this.loadUrl(urlInput?.value || '');
    });

    urlInput?.addEventListener('keypress', (e) => {
      if (e.key === 'Enter') {
        this.loadUrl(urlInput.value);
      }
    });
  }

  private async loadUrl(url: string): Promise<void> {
    if (!url) return;

    // Ensure URL has protocol
    if (!url.startsWith('http://') && !url.startsWith('https://')) {
      url = 'https://' + url;
    }

    this.currentUrl = url;
    console.log(`🌐 WebViewWidget: Loading URL: ${url}`);
    this.emitPositronContext();

    // Update URL input to show normalized URL
    const urlInput = this.shadowRoot?.querySelector('#urlInput') as HTMLInputElement;
    if (urlInput) {
      urlInput.value = url;
    }

    // Show loading state
    const contentDiv = this.shadowRoot?.querySelector('.browser-content');
    if (contentDiv) {
      contentDiv.innerHTML = `
        <div class="loading-state">
          <div class="loading-spinner"></div>
          <p>Fetching content...</p>
        </div>
      `;
    }

    try {
      // Fetch content via server-side command (bypasses CORS)
      const result = await this.executeCommand<WebFetchParams, WebFetchResult>(
        'interface/web/fetch',
        {
          url,
          format: 'markdown',
          maxLength: 100000
        }
      );

      if (!result.success) {
        throw new Error(result.error || 'Failed to fetch URL');
      }

      // Update page title in context
      if (result.title) {
        PositronWidgetState.emit(
          {
            widgetType: 'browser',
            title: result.title,
            metadata: {
              url: this.currentUrl,
              pageTitle: result.title,
              contentLength: result.contentLength
            }
          },
          { action: 'viewing', target: result.title }
        );
      }

      // Display fetched content
      if (contentDiv) {
        contentDiv.innerHTML = `
          <div class="fetched-content">
            ${result.title ? `<h1 class="page-title">${this.escapeHtml(result.title)}</h1>` : ''}
            <div class="markdown-content">${this.renderMarkdown(result.content)}</div>
          </div>
        `;
      }

      console.log(`✅ WebViewWidget: Loaded ${url} (${result.contentLength} chars)`);

    } catch (error) {
      console.error(`❌ WebViewWidget: Failed to load ${url}:`, error);

      if (contentDiv) {
        contentDiv.innerHTML = `
          <div class="error-state">
            <h2>Failed to load page</h2>
            <p class="error-url">${this.escapeHtml(url)}</p>
            <p class="error-message">${error instanceof Error ? this.escapeHtml(error.message) : 'Unknown error'}</p>
          </div>
        `;
      }
    }
  }

  /**
   * Simple HTML escaping
   */
  private escapeHtml(text: string): string {
    return text
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  /**
   * Render markdown to HTML (basic implementation)
   */
  private renderMarkdown(markdown: string): string {
    return markdown
      // Escape HTML first
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      // Then apply markdown formatting
      .replace(/^### (.*$)/gm, '<h3>$1</h3>')
      .replace(/^## (.*$)/gm, '<h2>$1</h2>')
      .replace(/^# (.*$)/gm, '<h1>$1</h1>')
      .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
      .replace(/\*(.*?)\*/g, '<em>$1</em>')
      .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank">$1</a>')
      .replace(/^- (.*$)/gm, '<li>$1</li>')
      .replace(/\n\n/g, '</p><p>')
      .replace(/\n/g, '<br>')
      // Wrap in paragraph
      .replace(/^/, '<p>')
      .replace(/$/, '</p>');
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('🌐 WebViewWidget: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
