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

  private loadUrl(url: string): void {
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

    // TODO: Fetch and display content via server-side proxy
    // For now, just update the placeholder
    const content = this.shadowRoot?.querySelector('.browser-content');
    if (content) {
      content.innerHTML = `
        <div class="placeholder-text">
          <h2>Loading...</h2>
          <p>${url}</p>
          <p>Server-side fetch coming soon.</p>
        </div>
      `;
    }
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('🌐 WebViewWidget: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
