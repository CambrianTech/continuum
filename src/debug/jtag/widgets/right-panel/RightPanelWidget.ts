/**
 * RightPanelWidget - Contextual right panel with AI assistant chat
 *
 * Shows contextual AI assistant chat based on the current view.
 * Embeds ChatWidget in compact mode - no duplicate chat code.
 *
 * Listens to UI_EVENTS.RIGHT_PANEL_CONFIGURE to update room/visibility
 * based on the current content type's recipe layout.
 *
 * Uses ReactiveWidget with Lit templates for efficient rendering.
 */

import {
  ReactiveWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../shared/ReactiveWidget';
import { Events } from '../../system/core/shared/Events';
import { UI_EVENTS, type RightPanelConfigPayload } from '../../system/core/shared/EventConstants';
import { styles as SIDE_PANEL_STYLES } from '../shared/styles/side-panel.styles';

export class RightPanelWidget extends ReactiveWidget {
  // Static styles
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(SIDE_PANEL_STYLES),
    unsafeCSS(`
      :host {
        clip-path: inset(0);
        box-sizing: border-box;
        min-width: 0;
      }

      .panel-content {
        position: absolute;
        top: 35px;
        left: 0;
        right: 0;
        bottom: 0;
        overflow: hidden;
        clip-path: inset(0);
        box-sizing: border-box;
      }

      .chat-container {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        overflow: hidden;
        clip-path: inset(0);
        box-sizing: border-box;
      }

      chat-widget {
        display: block;
        position: absolute;
        top: 0;
        left: 0;
        width: 100%;
        height: 100%;
        min-width: 0;
        overflow: hidden;
        clip-path: inset(0);
        box-sizing: border-box;
      }
    `)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private currentRoom: string = 'help';
  @reactive() private isHidden: boolean = false;

  // Non-reactive state (internal)
  private _eventUnsubscribe?: () => void;
  private _chatWidgetCache: HTMLElement | null = null;

  constructor() {
    super({
      widgetName: 'RightPanelWidget'
    });
  }

  // === Panel Configuration ===

  protected get panelTitle(): string {
    return 'Assistant';
  }

  protected get panelIcon(): string {
    return '🤖';
  }

  protected get panelSide(): 'left' | 'right' {
    return 'right';
  }

  protected get collapseChar(): string {
    return '»';
  }

  // === Lifecycle ===

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();
    this.log('Initializing...');

    // Listen for layout configuration events from MainWidget
    this.createMountEffect(() => {
      const unsubscribe = Events.subscribe(UI_EVENTS.RIGHT_PANEL_CONFIGURE, (config: RightPanelConfigPayload) => {
        this.handleLayoutConfig(config);
      });
      return () => unsubscribe();
    });

    this.log('Initialized with layout event listener');
  }

  // === Render ===

  protected override renderContent(): TemplateResult {
    return html`
      <div class="panel-header">
        <div class="panel-title">
          <span class="panel-title-icon">${this.panelIcon}</span>
          <span>${this.panelTitle}</span>
        </div>
        <button class="collapse-btn" title="Collapse panel" @click=${this.handleCollapse}>
          ${this.collapseChar}
        </button>
      </div>
      <div class="panel-content">
        <div class="chat-container">
          ${this.renderChatWidget()}
        </div>
      </div>
    `;
  }

  /**
   * Render the chat widget - cached to preserve state across room changes
   * Only recreates if room changes (handled via attribute update)
   */
  private renderChatWidget(): HTMLElement {
    if (!this._chatWidgetCache) {
      const chatWidget = document.createElement('chat-widget');
      chatWidget.setAttribute('compact', '');
      chatWidget.setAttribute('room', this.currentRoom);
      this._chatWidgetCache = chatWidget;
      this.log(`Created chat-widget for room '${this.currentRoom}'`);
    }
    return this._chatWidgetCache;
  }

  // === Layout Configuration ===

  /**
   * Handle layout configuration from MainWidget
   * Updates room and visibility based on content type's recipe
   */
  private handleLayoutConfig(config: RightPanelConfigPayload): void {
    this.log(`Received layout config for ${config.contentType}:`, config);

    if (config.widget === null) {
      // Hide the panel
      this.isHidden = true;
      this.collapse();
      this.requestUpdate();
      this.log(`Hiding panel for ${config.contentType}`);
    } else {
      // Show the panel with configured room
      this.isHidden = false;
      const newRoom = config.room || 'help';

      if (this.currentRoom !== newRoom) {
        this.currentRoom = newRoom;
        this.updateEmbeddedChat();
        this.requestUpdate();
        this.log(`Switched to room '${newRoom}' for ${config.contentType}`);
      }

      // Expand if it was hidden before
      if (!this.isHidden) {
        this.expand();
      }
    }
  }

  private updateEmbeddedChat(): void {
    if (this._chatWidgetCache) {
      this._chatWidgetCache.setAttribute('room', this.currentRoom);
    }
  }

  // === Collapse/Expand via PanelResizer ===

  private handleCollapse = (): void => {
    const continuumWidget = document.querySelector('continuum-widget');
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as any;
      resizer?.toggle?.();
    }
  };

  private collapse(): void {
    const continuumWidget = document.querySelector('continuum-widget');
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as any;
      resizer?.collapse?.();
    }
  }

  private expand(): void {
    const continuumWidget = document.querySelector('continuum-widget');
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as any;
      resizer?.expand?.();
    }
  }
}

// Register the custom element
// Registration handled by centralized BROWSER_WIDGETS registry
