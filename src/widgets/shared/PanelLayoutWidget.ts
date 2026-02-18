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
} from './ReactiveWidget';
import { ALL_PANEL_STYLES } from './styles';

export class PanelLayoutWidget extends ReactiveWidget {
  // Static styles
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(`
      ${ALL_PANEL_STYLES}

      :host {
        display: block;
        width: 100%;
        height: 100%;
      }

      ::slotted(*) {
        display: block;
      }
    `)
  ] as CSSResultGroup;

  // Reactive state from attributes
  @reactive() private panelTitle: string = 'Panel';
  @reactive() private panelSubtitle: string | null = null;

  // Observed attributes
  static override get observedAttributes(): string[] {
    return [...super.observedAttributes, 'title', 'subtitle'];
  }

  constructor() {
    super({
      widgetName: 'PanelLayoutWidget'
    });
  }

  override attributeChangedCallback(name: string, oldValue: string | null, newValue: string | null): void {
    super.attributeChangedCallback(name, oldValue, newValue);
    if (name === 'title' && newValue !== oldValue) {
      this.panelTitle = newValue || 'Panel';
      this.requestUpdate();
    } else if (name === 'subtitle' && newValue !== oldValue) {
      this.panelSubtitle = newValue;
      this.requestUpdate();
    }
  }

  // === Render ===

  protected override renderContent(): TemplateResult {
    return html`
      <div class="panel-layout">
        <div class="panel-main">
          <div class="panel-container">
            <div class="panel-header">
              <h1 class="panel-title">${this.panelTitle}</h1>
              ${this.panelSubtitle ? html`<p class="panel-subtitle">${this.panelSubtitle}</p>` : ''}
            </div>
            <slot></slot>
          </div>
        </div>
      </div>
    `;
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
