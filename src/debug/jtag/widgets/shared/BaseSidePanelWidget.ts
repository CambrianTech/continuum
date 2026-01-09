/**
 * BaseSidePanelWidget - Abstract base for collapsible side panels
 *
 * Consolidates common functionality between left sidebar and right panel:
 * - Header with title, icon, and collapse button
 * - Collapse/expand via PanelResizer (single source of truth)
 * - Proper flexbox/overflow for scrolling content
 * - Common styling patterns via CSS variables
 *
 * Uses ReactiveWidget with Lit templates for efficient rendering.
 *
 * Subclasses implement:
 * - panelTitle, panelIcon, panelSide
 * - renderPanelContent() - returns TemplateResult for the panel's content
 * - onPanelInitialize(), onPanelCleanup()
 */

import {
  ReactiveWidget,
  html,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from './ReactiveWidget';
import { styles as SIDE_PANEL_STYLES } from './styles/side-panel.styles';

export type SidePanelSide = 'left' | 'right';

export interface SidePanelConfig {
  widgetName: string;
  title?: string;
  icon?: string;
  side?: SidePanelSide;
}

export abstract class BaseSidePanelWidget extends ReactiveWidget {
  // Static styles using compiled SCSS
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(SIDE_PANEL_STYLES)
  ] as CSSResultGroup;

  constructor(config: SidePanelConfig) {
    super({
      widgetName: config.widgetName
    });
  }

  // === ABSTRACT PANEL CONFIGURATION ===

  /** Panel title text */
  protected abstract get panelTitle(): string;

  /** Panel icon (emoji or text) */
  protected abstract get panelIcon(): string;

  /** Panel side: 'left' or 'right' */
  protected abstract get panelSide(): SidePanelSide;

  /** Whether to show the header bar (default: true). Override to false for minimal header. */
  protected get showHeader(): boolean {
    return true;
  }

  /** Collapse button character - « for left, » for right */
  protected get collapseChar(): string {
    return this.panelSide === 'left' ? '«' : '»';
  }

  // === ABSTRACT LIFECYCLE ===

  /** Panel-specific initialization - called in onFirstRender */
  protected abstract onPanelInitialize(): Promise<void>;

  /** Panel-specific cleanup - called in onDisconnect */
  protected abstract onPanelCleanup(): Promise<void>;

  /** Render the panel's main content - returns TemplateResult */
  protected abstract renderPanelContent(): Promise<string>;

  /** Called after render - subclasses can override for post-render setup */
  protected async onPanelRendered(): Promise<void> {
    // Default: nothing
  }

  /** Override to add custom styles - subclasses return CSS string */
  protected getAdditionalStyles(): string {
    return '';
  }

  // === LIFECYCLE ===

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();
    await this.onPanelInitialize();
    await this.onPanelRendered();
  }

  protected override onDisconnect(): void {
    super.onDisconnect();
    this.onPanelCleanup().catch(err => console.error('Panel cleanup error:', err));
  }

  // === RENDER ===

  protected override renderContent(): TemplateResult {
    const additionalStyles = this.getAdditionalStyles();

    if (this.showHeader) {
      return html`
        ${additionalStyles ? html`<style>${additionalStyles}</style>` : ''}
        <div class="panel-header">
          <div class="panel-title">
            <span class="panel-title-icon">${this.panelIcon}</span>
            <span>${this.panelTitle}</span>
          </div>
          <button class="collapse-btn" title="Collapse panel" @click=${this.toggleCollapse}>
            ${this.collapseChar}
          </button>
        </div>
        <div class="panel-content" id="panel-content"></div>
      `;
    } else {
      // Minimal header: just floating collapse button
      return html`
        ${additionalStyles ? html`<style>${additionalStyles}</style>` : ''}
        <button class="collapse-btn floating" title="Collapse panel" @click=${this.toggleCollapse}>
          ${this.collapseChar}
        </button>
        <div class="panel-content full-height" id="panel-content"></div>
      `;
    }
  }

  /**
   * Called after first render to inject panel content
   * Uses innerHTML because subclasses still return string (gradual migration)
   */
  protected override async firstUpdated(): Promise<void> {
    super.firstUpdated();

    // Inject panel content (still string-based for backwards compatibility)
    const contentContainer = this.shadowRoot?.querySelector('#panel-content');
    if (contentContainer) {
      const content = await this.renderPanelContent();
      contentContainer.innerHTML = content;
    }
  }

  // === COLLAPSE/EXPAND VIA PANELRESIZER ===

  protected toggleCollapse = (): void => {
    const continuumWidget = document.querySelector('continuum-widget');
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as any;
      resizer?.toggle?.();
    }
  };

  protected collapse(): void {
    const continuumWidget = document.querySelector('continuum-widget');
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as any;
      resizer?.collapse?.();
    }
  }

  protected expand(): void {
    const continuumWidget = document.querySelector('continuum-widget');
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as any;
      resizer?.expand?.();
    }
  }
}
