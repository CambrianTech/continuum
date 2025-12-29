/**
 * WebViewWidget - Co-browsing web content with AI context
 *
 * Displays web content and emits context to PositronWidgetState
 * so AI assistants can "see" what the user is viewing.
 *
 * Now using ReactiveWidget for efficient rendering:
 * - No innerHTML replacement (preserves focus)
 * - Reactive state triggers minimal DOM updates
 * - Declarative event binding
 */

import { ReactiveWidget, html, css } from '../shared/ReactiveWidget';
import type { TemplateResult, CSSResultGroup } from '../shared/ReactiveWidget';
import type { WebFetchParams, WebFetchResult } from '@commands/interface/web/fetch/shared/WebFetchTypes';

interface FetchedPage {
  url: string;
  title?: string;
  content: string;
  contentLength: number;
  renderedHtml?: string; // Cache rendered markdown
}

export class WebViewWidget extends ReactiveWidget {
  // ═══════════════════════════════════════════════════════════════════════════
  // REACTIVE STATE
  // ═══════════════════════════════════════════════════════════════════════════

  static override properties = {
    ...ReactiveWidget.properties,
    urlInput: { type: String, state: true },
    currentUrl: { type: String, state: true },
    pageData: { type: Object, state: true },
    fetchError: { type: String, state: true }
  };

  protected urlInput = '';
  protected currentUrl = '';
  protected pageData: FetchedPage | null = null;
  protected fetchError: string | null = null;

  // ═══════════════════════════════════════════════════════════════════════════
  // STYLES
  // ═══════════════════════════════════════════════════════════════════════════

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
      :host {
        display: block;
        width: 100%;
        height: 100%;
        overflow: hidden;
        user-select: text;
        -webkit-user-select: text;
      }

      .browser-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        background: var(--bg-panel, rgba(20, 25, 35, 0.95));
      }

      .browser-toolbar {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 12px 16px;
        background: var(--bg-darker, rgba(10, 14, 20, 0.95));
        border-bottom: 1px solid var(--border-color, rgba(0, 212, 255, 0.2));
      }

      .url-input {
        flex: 1;
        padding: 8px 12px;
        background: rgba(255, 255, 255, 0.05);
        border: 1px solid var(--border-color, rgba(0, 212, 255, 0.2));
        border-radius: 4px;
        color: var(--color-text, #e0e0e0);
        font-size: 14px;
        font-family: monospace;
      }

      .url-input:focus {
        outline: none;
        border-color: var(--color-primary, #00d4ff);
        box-shadow: 0 0 8px rgba(0, 212, 255, 0.2);
      }

      .url-input::placeholder {
        color: var(--color-text-muted, #666);
      }

      .go-button {
        padding: 8px 16px;
        background: var(--color-primary, #00d4ff);
        border: none;
        border-radius: 4px;
        color: var(--bg-darker, #0a0e14);
        font-weight: 600;
        font-size: 14px;
        cursor: pointer;
        transition: all 0.2s;
      }

      .go-button:hover {
        box-shadow: 0 0 12px rgba(0, 212, 255, 0.4);
      }

      .go-button:active {
        transform: scale(0.98);
      }

      .go-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }

      .browser-content {
        flex: 1;
        overflow-y: auto;
        padding: 16px;
        color: var(--color-text, #e0e0e0);
        font-size: 14px;
        line-height: 1.6;
        user-select: text;
        -webkit-user-select: text;
        cursor: text;
      }

      .placeholder {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        text-align: center;
        color: var(--color-text-muted, #666);
      }

      .placeholder h2 {
        color: var(--color-primary, #00d4ff);
        font-size: 24px;
        margin: 0 0 16px 0;
        text-shadow: 0 0 8px rgba(0, 212, 255, 0.3);
      }

      .placeholder p {
        margin: 4px 0;
      }

      .fetched-content {
        max-width: 900px;
        margin: 0 auto;
        user-select: text;
        -webkit-user-select: text;
      }

      .page-title {
        color: var(--color-primary, #00d4ff);
        font-size: 28px;
        margin: 0 0 24px 0;
        padding-bottom: 12px;
        border-bottom: 1px solid var(--border-color, rgba(0, 212, 255, 0.2));
        text-shadow: 0 0 4px rgba(0, 212, 255, 0.2);
        user-select: text;
        -webkit-user-select: text;
      }

      .markdown-content h1,
      .markdown-content h2,
      .markdown-content h3 {
        color: var(--color-primary, #00d4ff);
        margin-top: 24px;
        margin-bottom: 12px;
      }

      .markdown-content h1 { font-size: 24px; }
      .markdown-content h2 { font-size: 20px; }
      .markdown-content h3 { font-size: 18px; }

      .markdown-content p {
        margin-bottom: 12px;
      }

      .markdown-content a {
        color: var(--color-primary, #00d4ff);
        text-decoration: none;
      }

      .markdown-content a:hover {
        text-decoration: underline;
      }

      .markdown-content strong {
        color: var(--color-text, #e0e0e0);
        font-weight: 600;
      }

      .markdown-content li {
        margin-left: 16px;
        margin-bottom: 4px;
      }

      .error-display {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        text-align: center;
        padding: 32px;
      }

      .error-display h2 {
        color: var(--color-error, #ff5050);
        margin: 0 0 16px 0;
        font-size: 24px;
      }

      .error-display .error-url {
        color: var(--color-text-muted, #666);
        font-family: monospace;
        word-break: break-all;
        margin: 0 0 12px 0;
      }

      .error-display .error-message {
        color: var(--color-error, #ff5050);
      }
    `
  ];

  // ═══════════════════════════════════════════════════════════════════════════
  // LIFECYCLE
  // ═══════════════════════════════════════════════════════════════════════════

  constructor() {
    super({
      widgetName: 'WebViewWidget',
      enableCommands: true,
      enablePositron: true,
      debug: false
    });
  }

  protected onFirstRender(): void {
    this.emitContext(
      { widgetType: 'browser', title: 'Web Browser' },
      { action: 'viewing', target: 'browser' }
    );
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // RENDER
  // ═══════════════════════════════════════════════════════════════════════════

  protected renderContent(): TemplateResult {
    return html`
      <div class="browser-container">
        <div class="browser-toolbar">
          <input
            type="text"
            class="url-input"
            placeholder="Enter URL..."
            .value=${this.urlInput}
            @input=${this.handleUrlInput}
            @keypress=${this.handleKeyPress}
          />
          <button
            class="go-button"
            ?disabled=${this.loading || !this.urlInput.trim()}
            @click=${this.handleGo}
          >
            ${this.loading ? 'Loading...' : 'Go'}
          </button>
        </div>
        <div class="browser-content">
          ${this.renderBrowserContent()}
        </div>
      </div>
    `;
  }

  private renderBrowserContent(): TemplateResult {
    // Loading state
    if (this.loading) {
      return this.renderLoading();
    }

    // Error state
    if (this.fetchError) {
      return html`
        <div class="error-display">
          <h2>Failed to load page</h2>
          <p class="error-url">${this.currentUrl}</p>
          <p class="error-message">${this.fetchError}</p>
        </div>
      `;
    }

    // Content loaded
    if (this.pageData) {
      // Use cached rendered HTML to avoid re-rendering markdown on every update
      const renderedContent = this.pageData.renderedHtml || this.pageData.content;
      return html`
        <div class="fetched-content">
          ${this.pageData.title ? html`<h1 class="page-title">${this.pageData.title}</h1>` : ''}
          <div class="markdown-content" .innerHTML=${renderedContent}></div>
        </div>
      `;
    }

    // Empty state
    return html`
      <div class="placeholder">
        <h2>Co-Browsing Widget</h2>
        <p>Enter a URL above to load web content.</p>
        <p>AI assistants will be able to see what you're viewing.</p>
      </div>
    `;
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // EVENT HANDLERS
  // ═══════════════════════════════════════════════════════════════════════════

  private handleUrlInput(e: InputEvent): void {
    this.urlInput = (e.target as HTMLInputElement).value;
  }

  private handleKeyPress(e: KeyboardEvent): void {
    if (e.key === 'Enter' && this.urlInput.trim()) {
      this.loadUrl();
    }
  }

  private handleGo(): void {
    if (this.urlInput.trim()) {
      this.loadUrl();
    }
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // URL LOADING
  // ═══════════════════════════════════════════════════════════════════════════

  private async loadUrl(): Promise<void> {
    let url = this.urlInput.trim();
    if (!url) return;

    // Ensure URL has protocol
    if (!url.startsWith('http://') && !url.startsWith('https://')) {
      url = 'https://' + url;
    }

    this.currentUrl = url;
    this.urlInput = url;
    this.fetchError = null;
    this.pageData = null;

    console.log(`🌐 WebViewWidget: Loading ${url}`);

    try {
      await this.withLoading(async () => {
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

        // Guard against undefined content
        const content = result.content || '';

        // Render markdown once and cache it
        const renderedHtml = this.renderMarkdown(content);

        this.pageData = {
          url,
          title: result.title,
          content,
          contentLength: content.length,
          renderedHtml
        };

        // Emit context for AI awareness
        this.emitContext(
          {
            widgetType: 'browser',
            title: result.title || 'Web Browser',
            metadata: {
              url,
              pageTitle: result.title,
              contentLength: result.contentLength
            }
          },
          { action: 'viewing', target: result.title || url }
        );

        console.log(`✅ WebViewWidget: Loaded ${url} (${content.length} chars)`);
      });
    } catch (e) {
      this.fetchError = e instanceof Error ? e.message : 'Unknown error';
      console.error(`❌ WebViewWidget: Failed to load ${url}:`, e);
    }
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // UTILITIES
  // ═══════════════════════════════════════════════════════════════════════════

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
      .replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>')
      .replace(/^- (.*$)/gm, '<li>$1</li>')
      .replace(/\n\n/g, '</p><p>')
      .replace(/\n/g, '<br>')
      .replace(/^/, '<p>')
      .replace(/$/, '</p>');
  }
}

// Register custom element
customElements.define('web-view-widget', WebViewWidget);

declare global {
  interface HTMLElementTagNameMap {
    'web-view-widget': WebViewWidget;
  }
}
