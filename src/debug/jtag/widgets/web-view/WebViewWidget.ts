/**
 * WebViewWidget - Co-browsing web content with AI context
 *
 * Displays web content via iframe through our proxy server.
 * The proxy:
 * - Bypasses X-Frame-Options by serving from our origin
 * - Rewrites URLs so navigation stays within proxy
 * - Injects JTAG shim for remote control (screenshot, click, type, etc.)
 *
 * AI assistants can "see" what the user is viewing via Positron context.
 * Tab name updates dynamically based on page title.
 */

import { ReactiveWidget, html, css } from '../shared/ReactiveWidget';
import type { TemplateResult, CSSResultGroup } from '../shared/ReactiveWidget';
import { Events } from '../../system/core/shared/Events';
import type { ConnectionStatus } from '../../system/core/client/browser/ConnectionMonitor';

const STORAGE_KEY = 'webview-widget-url';

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
    siteName: { type: String, state: true },
    loadError: { type: String, state: true },
    isLoading: { type: Boolean, state: true },
    isServerDisconnected: { type: Boolean, state: true },
    frozenScreenshot: { type: String, state: true }  // Screenshot to show when frozen
  };

  protected urlInput = '';
  protected currentUrl = '';
  protected proxyUrl = '';
  protected pageTitle = '';
  protected siteName = '';  // Short name for tab (e.g., "CNN", "GitHub")
  protected loadError = '';
  protected isLoading = false;
  protected isServerDisconnected = false;
  protected frozenScreenshot = '';  // Data URL of frozen state

  // Saved proxy URL for reconnection
  private savedProxyUrl = '';
  private connectionUnsubscribe?: () => void;
  private navigationUnsubscribe?: () => void;
  private boundHandleShimMessage?: (event: MessageEvent) => void;

  // ═══════════════════════════════════════════════════════════════════════════
  // STYLES
  // ═══════════════════════════════════════════════════════════════════════════

  static override styles: CSSResultGroup = [
    ReactiveWidget.styles,
    css`
      :host {
        display: flex;
        flex-direction: column;
        flex: 1;
        width: 100%;
        height: 100%;
        min-height: 0;
        min-width: 0;
        overflow: hidden;
        user-select: text;
        -webkit-user-select: text;
      }

      .browser-container {
        display: flex;
        flex-direction: column;
        flex: 1;
        height: 100%;
        min-height: 0;
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

      .disconnected-overlay {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        background: var(--bg-panel, rgba(20, 25, 35, 0.98));
        z-index: 100;
      }

      .disconnected-overlay h2 {
        color: #ff8080;
        font-size: 20px;
        margin: 0 0 12px 0;
      }

      .disconnected-overlay p {
        color: var(--color-text-muted, #666);
        font-size: 14px;
        margin: 4px 0;
      }

      .reconnecting-spinner {
        width: 24px;
        height: 24px;
        border: 3px solid var(--color-primary, #00d4ff);
        border-top-color: transparent;
        border-radius: 50%;
        animation: spin 1s linear infinite;
        margin-bottom: 16px;
      }

      @keyframes spin {
        to { transform: rotate(360deg); }
      }

      .freeze-info {
        background: rgba(20, 25, 35, 0.85);
        padding: 24px 40px;
        border-radius: 12px;
        border: 1px solid var(--border-accent, rgba(0, 212, 255, 0.3));
        text-align: center;
        backdrop-filter: blur(8px);
      }

      .freeze-info h2 {
        margin: 12px 0 8px 0;
      }

      .freeze-info p {
        margin: 4px 0;
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
    // Register with Positronic state system for RAG context injection
    this.registerWidgetState({
      url: '',
      pageTitle: '',
      siteName: '',
      isLoading: false,
      isServerDisconnected: false
    });

    // Subscribe to connection status to handle server restarts
    this.connectionUnsubscribe = Events.subscribe('connection:status', (status: ConnectionStatus) => {
      this.handleConnectionStatus(status);
    });

    // Subscribe to navigation requests (from navigate command with target='webview')
    this.navigationUnsubscribe = Events.subscribe('webview:navigate', (data: { url: string }) => {
      this.navigateToUrl(data.url);
    });

    // Listen for URL changes from the iframe shim (for URL bar sync)
    this.boundHandleShimMessage = this.handleShimMessage.bind(this);
    window.addEventListener('message', this.boundHandleShimMessage);

    // Check for pending navigation URL (set by navigate command before widget mounted)
    const pendingUrl = localStorage.getItem('webview:pending-url');
    if (pendingUrl) {
      console.log(`🌐 WebViewWidget: Found pending URL: ${pendingUrl}`);
      localStorage.removeItem('webview:pending-url');
      this.navigateToUrl(pendingUrl);
      return;  // Skip restoring saved URL - use pending instead
    }

    // Restore saved URL (from previous session)
    const savedUrl = localStorage.getItem(STORAGE_KEY);
    if (savedUrl) {
      this.urlInput = savedUrl;
      this.loadUrl();
    } else {
      this.emitContext(
        { widgetType: 'browser', title: 'Browser' },
        { action: 'viewing', target: 'browser' }
      );
    }
  }

  /**
   * Handle connection status changes - freeze iframe on disconnect
   * to prevent request flood when proxy server is down.
   * Captures screenshot of current state before clearing iframe.
   */
  private async handleConnectionStatus(status: ConnectionStatus): Promise<void> {
    console.log(`🔌 WebViewWidget: Connection status changed - connected: ${status.connected}, proxyUrl: ${!!this.proxyUrl}, isServerDisconnected: ${this.isServerDisconnected}`);

    if (!status.connected && this.proxyUrl) {
      // Server disconnected - capture screenshot and STOP iframe to prevent request flood
      console.log('⏸️ WebViewWidget: Server disconnected, freezing iframe...');

      const iframe = this.shadowRoot?.querySelector('.browser-frame') as HTMLIFrameElement;
      if (iframe?.contentWindow) {
        // STEP 1: Tell the shim to stop proxying (immediate, before any captures)
        console.log('🛑 WebViewWidget: Signaling shim to stop proxy requests');
        iframe.contentWindow.postMessage({ type: 'jtag-shim-freeze' }, '*');

        // STEP 2: Capture freeze frame while page content still exists (quick, 500ms timeout)
        await this.captureFreeze();

        // STEP 3: Clear iframe to absolutely stop all network activity
        console.log('🛑 WebViewWidget: Setting iframe src to about:blank');
        iframe.src = 'about:blank';
      }

      this.savedProxyUrl = this.proxyUrl;
      this.proxyUrl = '';  // This removes the iframe from DOM (overlay shows instead)
      this.isServerDisconnected = true;

      // Update Positronic state
      this.updateWidgetState({ isServerDisconnected: true });

      console.log('❄️ WebViewWidget: Iframe frozen, request flood prevented');
    } else if (status.connected && this.isServerDisconnected) {
      // Server reconnected - restore iframe
      console.log('▶️ WebViewWidget: Server reconnected, restoring iframe');
      this.isServerDisconnected = false;
      this.frozenScreenshot = '';  // Clear frozen state

      // Update Positronic state
      this.updateWidgetState({ isServerDisconnected: false, isLoading: true });

      if (this.savedProxyUrl) {
        this.proxyUrl = this.savedProxyUrl;
        this.savedProxyUrl = '';
        this.isLoading = true;
      }
    }
  }

  /**
   * Capture a screenshot of the current iframe state for freeze display
   */
  private async captureFreeze(): Promise<void> {
    const iframe = this.shadowRoot?.querySelector('.browser-frame') as HTMLIFrameElement;
    if (!iframe?.contentWindow) return;

    const requestId = `freeze-${Date.now()}`;

    try {
      const result = await new Promise<{ success: boolean; data?: { dataUrl: string } }>((resolve) => {
        // Short timeout since server is going down
        const timeout = setTimeout(() => {
          window.removeEventListener('message', handler);
          resolve({ success: false });
        }, 500);

        const handler = (event: MessageEvent) => {
          if (event.data?.type === 'jtag-shim-response' && event.data?.requestId === requestId) {
            clearTimeout(timeout);
            window.removeEventListener('message', handler);
            resolve(event.data.result);
          }
        };

        window.addEventListener('message', handler);
        iframe.contentWindow?.postMessage({
          type: 'jtag-shim-request',
          command: 'screenshot',
          params: { viewportOnly: true, quality: 0.5 },  // Lower quality for speed
          requestId
        }, '*');
      });

      if (result.success && result.data?.dataUrl) {
        this.frozenScreenshot = result.data.dataUrl;
        console.log('📸 WebViewWidget: Freeze frame captured');
      }
    } catch {
      // Silent fail - we'll show the disconnect overlay anyway
    }
  }

  override disconnectedCallback(): void {
    super.disconnectedCallback();
    // Clean up subscriptions
    if (this.connectionUnsubscribe) {
      this.connectionUnsubscribe();
      this.connectionUnsubscribe = undefined;
    }
    if (this.navigationUnsubscribe) {
      this.navigationUnsubscribe();
      this.navigationUnsubscribe = undefined;
    }
    if (this.boundHandleShimMessage) {
      window.removeEventListener('message', this.boundHandleShimMessage);
      this.boundHandleShimMessage = undefined;
    }
  }

  /**
   * Handle messages from the iframe shim (URL changes, ready status)
   */
  private handleShimMessage(event: MessageEvent): void {
    const data = event.data;
    if (!data || typeof data !== 'object') return;

    // URL change notification from shim
    if (data.type === 'jtag-shim-url-change') {
      this.handleUrlChangeFromIframe(data.url, data.title);
    }
  }

  /**
   * Update URL bar and state when navigation happens inside iframe
   */
  private handleUrlChangeFromIframe(proxiedUrl: string, pageTitle: string): void {
    // The URL from the shim is the proxied URL like /proxy/https%3A%2F%2Fwww.cnn.com%2F...
    // We need to extract the actual URL
    const actualUrl = this.extractActualUrl(proxiedUrl);
    if (!actualUrl) return;

    console.log(`🔗 WebViewWidget: URL changed in iframe to ${actualUrl}`);

    // Update state
    this.currentUrl = actualUrl;
    this.urlInput = actualUrl;
    this.pageTitle = pageTitle || '';
    this.siteName = this.extractSiteName(actualUrl, pageTitle);

    // Persist for next session
    localStorage.setItem(STORAGE_KEY, actualUrl);

    // Update Positronic state for RAG context
    this.updateWidgetState({
      url: actualUrl,
      pageTitle: this.pageTitle,
      siteName: this.siteName
    });

    // Emit updated context (legacy system)
    this.emitContext(
      {
        widgetType: 'browser',
        title: this.siteName || 'Browser',
        section: this.pageTitle,
        metadata: { url: actualUrl, pageTitle: this.pageTitle }
      },
      { action: 'viewing', target: actualUrl }
    );
  }

  /**
   * Extract the actual URL from a proxied URL
   * e.g., /proxy/https%3A%2F%2Fwww.cnn.com%2F → https://www.cnn.com/
   */
  private extractActualUrl(proxiedUrl: string): string {
    // Handle proxy prefix
    if (proxiedUrl.startsWith('/proxy/')) {
      const encoded = proxiedUrl.substring(7);  // Remove '/proxy/'
      try {
        return decodeURIComponent(encoded);
      } catch {
        return '';
      }
    }

    // Handle local URLs (shouldn't happen, but just in case)
    if (proxiedUrl.startsWith('http://localhost') || proxiedUrl.startsWith('/')) {
      return '';  // Ignore local navigation
    }

    // Return as-is if it's already a regular URL
    return proxiedUrl;
  }

  /**
   * Public method to navigate to a URL (called by navigate command)
   */
  public navigateToUrl(url: string): void {
    if (!url) return;
    console.log(`🌐 WebViewWidget: navigateToUrl called with ${url}`);
    this.urlInput = url;
    this.loadUrl();
    // Force Lit to re-render in case reactive update doesn't trigger
    this.requestUpdate();
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
            ?disabled=${!this.urlInput.trim() || this.isServerDisconnected}
            @click=${this.handleGo}
          >
            Go
          </button>
        </div>
        ${this.loadError ? html`
          <div class="error-banner">
            <strong>Load failed:</strong> ${this.loadError}
            <br><small>Some sites block proxied requests. Try documentation or blog sites instead.</small>
          </div>
        ` : ''}
        ${this.renderBrowserContent()}
        ${this.isServerDisconnected ? html`
          <div class="disconnected-overlay" style="${this.frozenScreenshot ? `background-image: url(${this.frozenScreenshot}); background-size: cover; background-position: top center;` : ''}">
            <div class="freeze-info">
              <div class="reconnecting-spinner"></div>
              <h2>Server Disconnected</h2>
              <p>Waiting for server to reconnect...</p>
            </div>
          </div>
        ` : ''}
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

  private async handleIframeLoad(): Promise<void> {
    this.isLoading = false;
    this.loadError = '';
    console.log(`✅ WebViewWidget: iframe loaded for ${this.currentUrl}`);

    // Update Positronic state
    this.updateWidgetState({ isLoading: false });

    // Query the shim for page info to get title
    await this.queryPageInfo();
  }

  /**
   * Query the injected JTAG shim for page info (title, url, dimensions)
   */
  private async queryPageInfo(): Promise<void> {
    const iframe = this.shadowRoot?.querySelector('.browser-frame') as HTMLIFrameElement;
    if (!iframe?.contentWindow) return;

    const requestId = `pageInfo-${Date.now()}`;

    try {
      const result = await new Promise<{ success: boolean; data?: { title: string; url: string } }>((resolve) => {
        const timeout = setTimeout(() => {
          window.removeEventListener('message', handler);
          resolve({ success: false });
        }, 5000);

        const handler = (event: MessageEvent) => {
          if (event.data?.type === 'jtag-shim-response' && event.data?.requestId === requestId) {
            clearTimeout(timeout);
            window.removeEventListener('message', handler);
            resolve(event.data.result);
          }
        };

        window.addEventListener('message', handler);
        iframe.contentWindow?.postMessage({
          type: 'jtag-shim-request',
          command: 'pageInfo',
          params: {},
          requestId
        }, '*');
      });

      if (result.success && result.data) {
        this.pageTitle = result.data.title || '';
        this.siteName = this.extractSiteName(result.data.url || this.currentUrl, result.data.title);
        this.updateTabTitle();

        // Update Positronic state for RAG context
        this.updateWidgetState({
          pageTitle: this.pageTitle,
          siteName: this.siteName
        });

        // Emit context (legacy system)
        this.emitContext(
          {
            widgetType: 'browser',
            title: this.siteName || 'Browser',
            section: this.pageTitle,
            metadata: { url: this.currentUrl, pageTitle: this.pageTitle }
          },
          { action: 'viewing', target: this.currentUrl }
        );
      }
    } catch (error) {
      console.warn('Failed to query page info:', error);
    }
  }

  /**
   * Extract a short site name from URL or title
   */
  private extractSiteName(url: string, title: string): string {
    try {
      const hostname = new URL(url).hostname;
      // Remove www. and get domain name
      const domain = hostname.replace(/^www\./, '').split('.')[0];
      // Capitalize first letter
      return domain.charAt(0).toUpperCase() + domain.slice(1);
    } catch {
      // Fallback: first word of title
      return title.split(/[\s\-|]/)[0] || 'Browser';
    }
  }

  /**
   * Update the tab title via event
   * TODO: Add TAB_TITLE_UPDATE event to EventConstants
   */
  private updateTabTitle(): void {
    // For now just log - tab system needs to listen for this
    console.log(`📑 WebViewWidget: Tab title would be "${this.siteName}" - "${this.pageTitle}"`);
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
    this.siteName = this.extractSiteName(url, '');

    // Persist URL for next session
    localStorage.setItem(STORAGE_KEY, url);

    // Create proxy URL
    this.proxyUrl = '/proxy/' + encodeURIComponent(url);

    console.log(`🌐 WebViewWidget: Loading ${url} via proxy, urlInput=${this.urlInput}, isLoading=${this.isLoading}`);

    // Update Positronic state for RAG context
    this.updateWidgetState({
      url,
      siteName: this.siteName,
      isLoading: true,
      pageTitle: ''  // Will be updated after load
    });

    // Force re-render to show URL in input and loading state
    this.requestUpdate();

    // Emit initial context (legacy system - will be updated with page title after load)
    this.emitContext(
      {
        widgetType: 'browser',
        title: this.siteName || 'Browser',
        metadata: { url, proxyUrl: this.proxyUrl }
      },
      { action: 'viewing', target: url }
    );

    // Fallback: check for load completion after delay
    // The @load event sometimes doesn't fire reliably with proxied content
    this.scheduleLoadCheck();
  }

  /**
   * Fallback mechanism to detect when iframe has loaded
   * Checks if shim is responsive, indicating page has loaded
   */
  private scheduleLoadCheck(): void {
    setTimeout(async () => {
      if (!this.isLoading) return;  // Already handled by @load event

      const iframe = this.shadowRoot?.querySelector('.browser-frame') as HTMLIFrameElement;
      if (!iframe?.contentWindow) return;

      // Try to ping the shim to see if page loaded
      const requestId = `loadcheck-${Date.now()}`;
      try {
        const result = await new Promise<boolean>((resolve) => {
          const timeout = setTimeout(() => {
            window.removeEventListener('message', handler);
            resolve(false);
          }, 2000);

          const handler = (event: MessageEvent) => {
            if (event.data?.type === 'jtag-shim-response' && event.data?.requestId === requestId) {
              clearTimeout(timeout);
              window.removeEventListener('message', handler);
              resolve(true);
            }
          };

          window.addEventListener('message', handler);
          iframe.contentWindow?.postMessage({
            type: 'jtag-shim-request',
            command: 'ping',
            params: {},
            requestId
          }, '*');
        });

        if (result && this.isLoading) {
          console.log('✅ WebViewWidget: Load detected via shim ping (fallback)');
          this.isLoading = false;
          await this.queryPageInfo();
        }
      } catch {
        // Ignore errors
      }
    }, 3000);  // Check after 3 seconds
  }
}

// Register custom element
customElements.define('web-view-widget', WebViewWidget);

declare global {
  interface HTMLElementTagNameMap {
    'web-view-widget': WebViewWidget;
  }
}
