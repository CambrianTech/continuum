/**
 * WebViewWidget - Co-browsing web content with AI context
 *
 * Displays web content and emits context to PositronWidgetState
 * so AI assistants can "see" what the user is viewing.
 */

import { BaseWidget } from '../shared/BaseWidget';
import { PositronWidgetState } from '../shared/services/state/PositronWidgetState';

export class WebViewWidget extends BaseWidget {
  private currentUrl: string = '';

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
    console.log('🌐 WebViewWidget: Initializing...');
    this.emitPositronContext();
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
    const styles = `
      :host {
        display: block;
        height: 100%;
        overflow: hidden;
      }

      .browser-container {
        display: flex;
        flex-direction: column;
        height: 100%;
        background: #2a2a2a;
      }

      .browser-toolbar {
        display: flex;
        align-items: center;
        gap: 8px;
        padding: 12px 16px;
        background: #1a1a1a;
        border-bottom: 1px solid rgba(0, 212, 255, 0.2);
      }

      .url-input {
        flex: 1;
        padding: 8px 12px;
        background: #333;
        border: 1px solid rgba(0, 212, 255, 0.3);
        border-radius: 4px;
        color: white;
        font-size: 14px;
      }

      .url-input:focus {
        outline: none;
        border-color: var(--content-accent, #00d4ff);
      }

      .go-button {
        padding: 8px 16px;
        background: var(--content-accent, #00d4ff);
        border: none;
        border-radius: 4px;
        color: #000;
        font-weight: 600;
        cursor: pointer;
      }

      .go-button:hover {
        opacity: 0.9;
      }

      .browser-content {
        flex: 1;
        display: flex;
        align-items: center;
        justify-content: center;
        color: rgba(255, 255, 255, 0.5);
        font-size: 16px;
      }

      .placeholder-text {
        text-align: center;
        padding: 40px;
      }

      .placeholder-text h2 {
        color: var(--content-accent, #00d4ff);
        margin-bottom: 16px;
      }
    `;

    const template = `
      <div class="browser-container">
        <div class="browser-toolbar">
          <input type="text" class="url-input" placeholder="Enter URL..." value="${this.currentUrl}">
          <button class="go-button">Go</button>
        </div>
        <div class="browser-content">
          <div class="placeholder-text">
            <h2>Co-Browsing Widget</h2>
            <p>Enter a URL above to load web content.</p>
            <p>AI assistants will be able to see what you're viewing.</p>
          </div>
        </div>
      </div>
    `;

    this.shadowRoot!.innerHTML = `<style>${styles}</style>${template}`;
    this.setupEventListeners();
  }

  private setupEventListeners(): void {
    const urlInput = this.shadowRoot?.querySelector('.url-input') as HTMLInputElement;
    const goButton = this.shadowRoot?.querySelector('.go-button');

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
    this.currentUrl = url;
    console.log(`🌐 WebViewWidget: Loading URL: ${url}`);
    this.emitPositronContext();
    // TODO: Fetch and display content via server-side proxy
  }

  protected async onWidgetCleanup(): Promise<void> {
    console.log('🌐 WebViewWidget: Cleanup complete');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
