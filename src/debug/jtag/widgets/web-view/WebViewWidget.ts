/**
 * WebViewWidget - Co-browsing web content with AI context
 *
 * Displays web content via iframe through our proxy server.
 * The proxy:
 * - Bypasses X-Frame-Options by serving from our origin
 * - Rewrites URLs so navigation stays within proxy
 * - Forwards browser headers to look authentic
 *
 * AI assistants can "see" what the user is viewing via Positron context.
 */

import { ReactiveWidget, html, css } from '../shared/ReactiveWidget';
import type { TemplateResult, CSSResultGroup } from '../shared/ReactiveWidget';

export class WebViewWidget extends ReactiveWidget {
  // ═══════════════════════════════════════════════════════════════════════════
  // REACTIVE STATE
  // ═══════════════════════════════════════════════════════════════════════════

  static override properties = {
    ...ReactiveWidget.properties,
    urlInput: { type: String, state: true },
    currentUrl: { type: String, state: true },
    proxyUrl: { type: String, state: true },
    pageTitle: { type: String, state: true },
    loadError: { type: String, state: true },
    isLoading: { type: Boolean, state: true }
  };

  protected urlInput = '';
  protected currentUrl = '';
  protected proxyUrl = '';
  protected pageTitle = '';
  protected loadError = '';
  protected isLoading = false;

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
        padding: 8px 12px;
        background: var(--bg-darker, rgba(10, 14, 20, 0.95));
        border-bottom: 1px solid var(--border-color, rgba(0, 212, 255, 0.2));
        flex-shrink: 0;
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

      .go-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
      }

      .browser-frame {
        flex: 1;
        border: none;
        background: white;
      }

      .placeholder {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        height: 100%;
        text-align: center;
        color: var(--color-text-muted, #666);
        padding: 32px;
      }

      .placeholder h2 {
        color: var(--color-primary, #00d4ff);
        font-size: 24px;
        margin: 0 0 16px 0;
        text-shadow: 0 0 8px rgba(0, 212, 255, 0.3);
      }

      .placeholder p {
        margin: 4px 0;
        max-width: 400px;
      }

      .current-url {
        font-size: 11px;
        color: var(--color-text-muted, #666);
        padding: 4px 12px;
        background: rgba(0, 0, 0, 0.2);
        border-bottom: 1px solid var(--border-color, rgba(0, 212, 255, 0.1));
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
      }

      .loading-overlay {
        position: absolute;
        top: 50%;
        left: 50%;
        transform: translate(-50%, -50%);
        color: var(--color-primary, #00d4ff);
        font-size: 18px;
        text-shadow: 0 0 8px rgba(0, 212, 255, 0.4);
      }

      .error-banner {
        padding: 12px 16px;
        background: rgba(255, 80, 80, 0.15);
        border-bottom: 1px solid rgba(255, 80, 80, 0.3);
        color: #ff8080;
        font-size: 13px;
      }

      .error-banner strong {
        color: #ff5050;
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
            placeholder="Enter URL... (e.g., https://example.com)"
            .value=${this.urlInput}
            @input=${this.handleUrlInput}
            @keypress=${this.handleKeyPress}
          />
          <button
            class="go-button"
            ?disabled=${!this.urlInput.trim()}
            @click=${this.handleGo}
          >
            Go
          </button>
        </div>
        ${this.currentUrl ? html`
          <div class="current-url">${this.currentUrl}</div>
        ` : ''}
        ${this.loadError ? html`
          <div class="error-banner">
            <strong>Load failed:</strong> ${this.loadError}
            <br><small>Some sites block proxied requests. Try documentation or blog sites instead.</small>
          </div>
        ` : ''}
        ${this.renderBrowserContent()}
      </div>
    `;
  }

  private renderBrowserContent(): TemplateResult {
    if (this.proxyUrl) {
      return html`
        <iframe
          class="browser-frame"
          src=${this.proxyUrl}
          @load=${this.handleIframeLoad}
          @error=${this.handleIframeError}
          sandbox="allow-same-origin allow-scripts allow-forms allow-popups"
        ></iframe>
        ${this.isLoading ? html`<div class="loading-overlay">Loading...</div>` : ''}
      `;
    }

    return html`
      <div class="placeholder">
        <h2>Co-Browsing Widget</h2>
        <p>Enter a URL above to browse web content.</p>
        <p><strong>Works well:</strong> Documentation, blogs, articles, most content sites</p>
        <p><strong>May not work:</strong> Google, Facebook, sites with bot detection</p>
        <p>AI assistants can see what you're viewing and provide contextual help.</p>
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

  private handleIframeLoad(): void {
    this.isLoading = false;
    this.loadError = '';
    console.log(`✅ WebViewWidget: iframe loaded for ${this.currentUrl}`);
  }

  private handleIframeError(): void {
    this.isLoading = false;
    this.loadError = 'Failed to load page';
    console.error(`❌ WebViewWidget: iframe error for ${this.currentUrl}`);
  }

  // ═══════════════════════════════════════════════════════════════════════════
  // URL LOADING
  // ═══════════════════════════════════════════════════════════════════════════

  private loadUrl(): void {
    let url = this.urlInput.trim();
    if (!url) return;

    // Ensure URL has protocol
    if (!url.startsWith('http://') && !url.startsWith('https://')) {
      url = 'https://' + url;
    }

    this.currentUrl = url;
    this.urlInput = url;
    this.loadError = '';
    this.isLoading = true;

    // Create proxy URL
    this.proxyUrl = '/proxy/' + encodeURIComponent(url);

    console.log(`🌐 WebViewWidget: Loading ${url} via proxy`);

    // Emit context for AI awareness
    this.emitContext(
      {
        widgetType: 'browser',
        title: 'Web Browser',
        metadata: {
          url,
          proxyUrl: this.proxyUrl
        }
      },
      { action: 'viewing', target: url }
    );
  }
}

// Register custom element
customElements.define('web-view-widget', WebViewWidget);

declare global {
  interface HTMLElementTagNameMap {
    'web-view-widget': WebViewWidget;
  }
}
