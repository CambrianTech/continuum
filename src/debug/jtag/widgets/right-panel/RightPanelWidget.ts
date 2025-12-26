/**
 * RightPanelWidget - Contextual right panel that embeds ChatWidget
 *
 * Shows contextual AI assistant chat based on the current view.
 * Embeds ChatWidget in compact mode - no duplicate chat code.
 * Can be collapsed/expanded.
 *
 * TODO: Wire to recipe system - each room's recipe defines layout.rightPanel
 * For now, starts collapsed until recipe-driven layout is implemented.
 */

import { BaseWidget } from '../shared/BaseWidget';

export class RightPanelWidget extends BaseWidget {
  private currentRoom: string = 'help';

  constructor() {
    super({
      widgetName: 'RightPanelWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: true,
      enableScreenshots: false
    });
  }

  protected async onWidgetInitialize(): Promise<void> {
    console.log('📋 RightPanelWidget: Initializing...');
    // Collapse state managed by RightPanelResizer (persisted to localStorage)
    console.log('✅ RightPanelWidget: Initialized');
  }

  /**
   * Toggle collapse via the resizer (single source of truth)
   */
  private toggleCollapse(): void {
    const continuumWidget = document.querySelector('continuum-widget') as any;
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector('right-panel-resizer') as any;
      if (resizer?.toggle) {
        resizer.toggle();
      }
    }
  }

  private updateEmbeddedChat(): void {
    const chatWidget = this.shadowRoot?.querySelector('chat-widget');
    if (chatWidget) {
      chatWidget.setAttribute('room', this.currentRoom);
    }
  }

  protected async renderWidget(): Promise<void> {
    const styles = `
      :host {
        display: flex;
        flex-direction: column;
        height: 100%;
        max-height: 100%;
        width: 100%;
        overflow: hidden;
        background: var(--sidebar-background, rgba(20, 25, 35, 0.95));
        color: var(--content-primary, #e0e6ed);
      }

      .panel-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 8px 12px;
        border-bottom: 1px solid var(--border-subtle, rgba(255,255,255,0.1));
        background: rgba(0, 0, 0, 0.2);
        flex-shrink: 0;
      }

      .panel-title {
        display: flex;
        align-items: center;
        gap: 6px;
        font-size: 13px;
        font-weight: 600;
        color: var(--content-accent, #00d4ff);
      }

      .panel-title-icon {
        font-size: 14px;
      }

      .collapse-btn {
        background: none;
        border: none;
        color: var(--content-secondary, #8a92a5);
        cursor: pointer;
        padding: 4px 8px;
        font-size: 14px;
        transition: color 0.2s;
      }

      .collapse-btn:hover {
        color: var(--content-accent, #00d4ff);
      }

      .chat-container {
        flex: 1;
        min-height: 0;
        display: flex;
        overflow: hidden;
      }

      chat-widget {
        flex: 1;
        width: 100%;
        height: 100%;
        max-height: 100%;
        overflow: hidden;
      }
    `;

    this.shadowRoot!.innerHTML = `
      <style>${styles}</style>
      <div class="panel-header">
        <div class="panel-title">
          <span class="panel-title-icon">🤖</span>
          <span>Assistant</span>
        </div>
        <button class="collapse-btn" title="Collapse panel">»</button>
      </div>
      <div class="chat-container">
        <chat-widget compact room="${this.currentRoom}"></chat-widget>
      </div>
    `;

    this.setupEventListeners();
  }

  private setupEventListeners(): void {
    const collapseBtn = this.shadowRoot?.querySelector('.collapse-btn');
    if (collapseBtn) {
      collapseBtn.addEventListener('click', () => {
        this.toggleCollapse();
      });
    }
  }

  protected async onWidgetCleanup(): Promise<void> {
    // Nothing to clean up - ChatWidget handles its own cleanup
  }
}

// Register the custom element
customElements.define('right-panel-widget', RightPanelWidget);
