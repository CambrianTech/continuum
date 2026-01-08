/**
 * RightPanelWidget - Contextual right panel with AI assistant chat
 *
 * Shows contextual AI assistant chat based on the current view.
 * Embeds ChatWidget in compact mode - no duplicate chat code.
 *
 * Listens to UI_EVENTS.RIGHT_PANEL_CONFIGURE to update room/visibility
 * based on the current content type's recipe layout.
 *
 * Extends BaseSidePanelWidget for consistent panel behavior.
 */

import { BaseSidePanelWidget, type SidePanelSide } from '../shared/BaseSidePanelWidget';
import { Events } from '../../system/core/shared/Events';
import { UI_EVENTS, type RightPanelConfigPayload } from '../../system/core/shared/EventConstants';

export class RightPanelWidget extends BaseSidePanelWidget {
  private currentRoom: string = 'help';
  private isHidden: boolean = false;
  private _eventUnsubscribe?: () => void;

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

  protected get panelSide(): SidePanelSide {
    return 'right';
  }

  // === Lifecycle ===

  protected async onPanelInitialize(): Promise<void> {
    this.verbose() && console.log('📋 RightPanelWidget: Initializing...');

    // Listen for layout configuration events from MainWidget
    this._eventUnsubscribe = Events.subscribe(UI_EVENTS.RIGHT_PANEL_CONFIGURE, (config: RightPanelConfigPayload) => {
      this.handleLayoutConfig(config);
    });

    this.verbose() && console.log('✅ RightPanelWidget: Initialized with layout event listener');
  }

  protected async onPanelCleanup(): Promise<void> {
    this._eventUnsubscribe?.();
  }

  // === Content Rendering ===

  protected async renderPanelContent(): Promise<string> {
    return `
      <div class="chat-container">
        <chat-widget compact room="${this.currentRoom}"></chat-widget>
      </div>
    `;
  }

  protected getAdditionalStyles(): string {
    return `
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
    `;
  }

  // === Layout Configuration ===

  /**
   * Handle layout configuration from MainWidget
   * Updates room and visibility based on content type's recipe
   */
  private handleLayoutConfig(config: RightPanelConfigPayload): void {
    this.verbose() && console.log(`📋 RightPanelWidget: Received layout config for ${config.contentType}:`, config);

    if (config.widget === null) {
      // Hide the panel
      this.isHidden = true;
      this.collapse();
      this.verbose() && console.log(`📋 RightPanelWidget: Hiding panel for ${config.contentType}`);
    } else {
      // Show the panel with configured room
      this.isHidden = false;
      const newRoom = config.room || 'help';

      if (this.currentRoom !== newRoom) {
        this.currentRoom = newRoom;
        this.updateEmbeddedChat();
        this.verbose() && console.log(`📋 RightPanelWidget: Switched to room '${newRoom}' for ${config.contentType}`);
      }

      // Expand if it was hidden before
      if (!this.isHidden) {
        this.expand();
      }
    }
  }

  private updateEmbeddedChat(): void {
    const chatWidget = this.shadowRoot?.querySelector('chat-widget');
    if (chatWidget) {
      chatWidget.setAttribute('room', this.currentRoom);
    }
  }
}

// Register the custom element
// Registration handled by centralized BROWSER_WIDGETS registry
