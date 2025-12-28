/**
 * PanelLayoutWidget - Composable panel layout container
 *
 * Provides single-column layout with header via slots.
 * AI assistance provided via unified right panel (Positron).
 * Widgets don't need to extend anything - just wrap them:
 *
 * ```html
 * <panel-layout title="Settings">
 *   <settings-widget></settings-widget>
 * </panel-layout>
 * ```
 *
 * Attributes:
 * - title: Panel header title
 * - subtitle: Panel header subtitle (optional)
 */

import { BaseWidget } from './BaseWidget';
import { ALL_PANEL_STYLES } from './styles';

export class PanelLayoutWidget extends BaseWidget {
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
    return ['title', 'subtitle'];
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
    // No cleanup needed
  }

  protected async renderWidget(): Promise<void> {
    if (!this.shadowRoot) return;

    const title = this.getAttribute('title') || 'Panel';
    const subtitle = this.getAttribute('subtitle');

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
      </div>
    `;
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
