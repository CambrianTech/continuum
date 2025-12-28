/**
 * PanelLayoutWidget - Composable panel layout container
 *
 * Provides two-column layout (content + assistant sidebar) via slots.
 * Widgets don't need to extend anything - just wrap them:
 *
 * ```html
 * <panel-layout title="Settings" assistant-room="settings">
 *   <settings-widget></settings-widget>
 * </panel-layout>
 * ```
 *
 * Attributes:
 * - title: Panel header title
 * - subtitle: Panel header subtitle (optional)
 * - assistant-room: Room name for AI assistant (optional, omit to hide assistant)
 * - assistant-greeting: Greeting message for assistant
 */

import { BaseWidget } from './BaseWidget';
import { AssistantPanel } from './AssistantPanel';
import { DEFAULT_ROOMS } from '../../system/data/domains/DefaultEntities';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { ALL_PANEL_STYLES } from './styles';

export class PanelLayoutWidget extends BaseWidget {
  private assistantPanel?: AssistantPanel;

  constructor() {
    super({
      widgetName: 'PanelLayoutWidget',
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  static get observedAttributes(): string[] {
    return ['title', 'subtitle', 'assistant-room', 'assistant-greeting'];
  }

  attributeChangedCallback(name: string, oldValue: string, newValue: string): void {
    if (oldValue !== newValue) {
      this.renderWidget();
    }
  }

  protected async onWidgetInitialize(): Promise<void> {
    // Initial render
  }

  protected async onWidgetCleanup(): Promise<void> {
    this.assistantPanel?.destroy();
  }

  protected async renderWidget(): Promise<void> {
    if (!this.shadowRoot) return;

    const title = this.getAttribute('title') || 'Panel';
    const subtitle = this.getAttribute('subtitle');
    const assistantRoom = this.getAttribute('assistant-room');
    const hasAssistant = !!assistantRoom;

    this.shadowRoot.innerHTML = `
      <style>
        ${ALL_PANEL_STYLES}
        
        :host {
          display: block;
          width: 100%;
          height: 100%;
        }
        
        ::slotted(*) {
          display: block;
        }
      </style>
      <div class="panel-layout">
        <div class="panel-main">
          <div class="panel-container">
            <div class="panel-header">
              <h1 class="panel-title">${title}</h1>
              ${subtitle ? `<p class="panel-subtitle">${subtitle}</p>` : ''}
            </div>
            <slot></slot>
          </div>
        </div>
        ${hasAssistant ? '<div class="panel-assistant" id="assistant-container"></div>' : ''}
      </div>
    `;

    if (hasAssistant) {
      this.initializeAssistant(assistantRoom);
    }
  }

  private initializeAssistant(roomName: string): void {
    if (!this.shadowRoot) return;

    const container = this.shadowRoot.querySelector('#assistant-container') as HTMLElement;
    if (!container) return;

    const roomId = (DEFAULT_ROOMS as any)[roomName.toUpperCase()] as UUID;
    if (!roomId) {
      console.warn(`PanelLayoutWidget: Room '${roomName}' not found in DEFAULT_ROOMS`);
      return;
    }

    const greeting = this.getAttribute('assistant-greeting') || 'How can I help?';

    this.assistantPanel = new AssistantPanel(container, {
      roomId,
      roomName,
      title: 'AI Assistant',
      placeholder: 'Ask for help...',
      greeting,
      maxMessages: 20,
      width: '320px',
      startCollapsed: false
    });
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
