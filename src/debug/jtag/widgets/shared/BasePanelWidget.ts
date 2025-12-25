/**
 * BasePanelWidget - Base class for panel-type widgets
 *
 * Extends BaseWidget with common patterns for Settings, Help, Theme, Diagnostics, etc.
 * These widgets share:
 * - Two-column layout (main content + optional AI assistant sidebar)
 * - Common styling (sections, headers, buttons, status indicators)
 * - AssistantPanel integration
 *
 * Usage:
 * ```typescript
 * export class MyPanelWidget extends BasePanelWidget {
 *   constructor() {
 *     super({
 *       widgetName: 'MyPanelWidget',
 *       panelTitle: 'My Panel',
 *       panelSubtitle: 'Description of what this panel does',
 *       assistantRoom: 'help',  // Optional: enables AI assistant sidebar
 *       assistantGreeting: 'How can I help?'
 *     });
 *   }
 *
 *   protected async renderContent(): Promise<string> {
 *     return `<div class="panel-section">...</div>`;
 *   }
 * }
 * ```
 */

import { BaseWidget, type WidgetConfig } from './BaseWidget';
import { AssistantPanel, type AssistantPanelConfig } from './AssistantPanel';
import { DEFAULT_ROOMS } from '../../system/data/domains/DefaultEntities';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';
import { ALL_PANEL_STYLES } from './styles';

export interface PanelWidgetConfig extends Partial<WidgetConfig> {
  /** Widget name (required) */
  widgetName: string;

  /** Panel title shown in header */
  panelTitle: string;

  /** Panel subtitle/description */
  panelSubtitle?: string;

  /** Room name for AI assistant (e.g., 'help', 'settings'). Omit to disable assistant. */
  assistantRoom?: string;

  /** AI assistant greeting message */
  assistantGreeting?: string;

  /** AI assistant placeholder text */
  assistantPlaceholder?: string;

  /** Start with assistant collapsed */
  assistantStartCollapsed?: boolean;

  /** Additional CSS styles (merged with panel styles) */
  additionalStyles?: string;

  /** Enable database access */
  enableDatabase?: boolean;
}

export abstract class BasePanelWidget extends BaseWidget {
  protected assistantPanel?: AssistantPanel;
  protected panelConfig: PanelWidgetConfig;

  constructor(config: PanelWidgetConfig) {
    // Build base config, ensuring widgetName is only specified once
    const baseConfig: Partial<WidgetConfig> = {
      widgetId: config.widgetName,
      widgetName: config.widgetName,
      template: undefined,
      styles: undefined,
      enableAI: false,
      enableDatabase: config.enableDatabase ?? false,
      enableRouterEvents: false,
      enableScreenshots: false
    };

    super(baseConfig as WidgetConfig);

    this.panelConfig = config;
  }

  /**
   * Implement BaseWidget abstract method - cleanup panel resources
   */
  protected async onWidgetCleanup(): Promise<void> {
    await this.onPanelCleanup();
  }

  /**
   * Override this to perform panel-specific cleanup
   */
  protected async onPanelCleanup(): Promise<void> {
    // Default: no cleanup needed
  }

  protected async onWidgetInitialize(): Promise<void> {
    await this.onPanelInitialize();
  }

  /**
   * Override this to perform panel-specific initialization
   */
  protected async onPanelInitialize(): Promise<void> {
    // Default: no additional initialization
  }

  /**
   * Override this to render the main content area
   * Return HTML string for the panel-main content
   */
  protected abstract renderContent(): Promise<string> | string;

  /**
   * Get CSS styles for this panel
   * Override to add custom styles (they will be merged with base panel styles)
   */
  protected getStyles(): string {
    return this.panelConfig.additionalStyles || '';
  }

  protected async renderWidget(): Promise<void> {
    if (!this.shadowRoot) return;

    const content = await this.renderContent();
    const hasAssistant = !!this.panelConfig.assistantRoom;

    this.shadowRoot.innerHTML = `
      <style>
        ${ALL_PANEL_STYLES}
        ${this.getStyles()}
      </style>
      <div class="panel-layout">
        <div class="panel-main">
          <div class="panel-container">
            <div class="panel-header">
              <h1 class="panel-title">${this.panelConfig.panelTitle}</h1>
              ${this.panelConfig.panelSubtitle ? `<p class="panel-subtitle">${this.panelConfig.panelSubtitle}</p>` : ''}
            </div>
            ${content}
          </div>
        </div>
        ${hasAssistant ? '<div class="panel-assistant" id="assistant-container"></div>' : ''}
      </div>
    `;

    // Initialize assistant panel if configured
    if (hasAssistant) {
      this.initializeAssistant();
    }

    // Call post-render hook
    await this.onContentRendered();
  }

  /**
   * Override this to perform actions after content is rendered
   * (e.g., attach event listeners to rendered elements)
   */
  protected async onContentRendered(): Promise<void> {
    // Default: no post-render actions
  }

  private initializeAssistant(): void {
    if (!this.shadowRoot || !this.panelConfig.assistantRoom) return;

    const container = this.shadowRoot.querySelector('#assistant-container') as HTMLElement;
    if (!container) return;

    // Get room ID from room name
    const roomId = (DEFAULT_ROOMS as any)[this.panelConfig.assistantRoom.toUpperCase()] as UUID;

    if (!roomId) {
      console.warn(`BasePanelWidget: Room '${this.panelConfig.assistantRoom}' not found in DEFAULT_ROOMS`);
      return;
    }

    this.assistantPanel = new AssistantPanel(container, {
      roomId,
      roomName: this.panelConfig.assistantRoom,
      title: 'AI Assistant',
      placeholder: this.panelConfig.assistantPlaceholder || 'Ask for help...',
      greeting: this.panelConfig.assistantGreeting || 'How can I help?',
      maxMessages: 20,
      width: '320px',
      startCollapsed: this.panelConfig.assistantStartCollapsed ?? false
    });
  }

  /**
   * Helper to create a section with title and content
   */
  protected createSection(title: string, content: string, options?: {
    highlighted?: boolean;
    intro?: string;
  }): string {
    const classes = ['panel-section'];
    if (options?.highlighted) classes.push('highlighted');

    return `
      <div class="${classes.join(' ')}">
        <h2 class="section-title">${title}</h2>
        ${options?.intro ? `<p class="section-intro">${options.intro}</p>` : ''}
        ${content}
      </div>
    `;
  }

  /**
   * Helper to create an info box
   */
  protected createInfoBox(content: string, type?: 'success' | 'warning' | 'error'): string {
    const classes = ['info-box'];
    if (type) classes.push(type);

    return `<div class="${classes.join(' ')}">${content}</div>`;
  }

  /**
   * Helper to create a loading state
   */
  protected createLoading(message?: string): string {
    return `
      <div class="loading">
        <div class="loading-spinner"></div>
        ${message ? `<p>${message}</p>` : ''}
      </div>
    `;
  }

  /**
   * Helper to create a status indicator
   */
  protected createStatusIndicator(status: string, label: string): string {
    const statusClass = status.toLowerCase().replace(/[^a-z]/g, '-');
    return `<span class="status-indicator status-${statusClass}">${label}</span>`;
  }
}
