/**
 * BaseSidePanelWidget - Abstract base for collapsible side panels
 *
 * Consolidates common functionality between left sidebar and right panel:
 * - Header with title, icon, and collapse button
 * - Collapse/expand via PanelResizer (single source of truth)
 * - Proper flexbox/overflow for scrolling content
 * - Common styling patterns via CSS variables
 *
 * Different from BasePanelWidget which is for content panels (Settings, Help, Theme).
 * This is specifically for the collapsible sidebars.
 *
 * Subclasses implement:
 * - panelTitle, panelIcon, panelSide
 * - renderPanelContent() - the panel's main content
 * - onPanelInitialize(), onPanelCleanup()
 */

import { BaseWidget, type WidgetConfig } from './BaseWidget';

export type SidePanelSide = 'left' | 'right';

export interface SidePanelConfig extends Partial<WidgetConfig> {
  title?: string;
  icon?: string;
  side?: SidePanelSide;
}

export abstract class BaseSidePanelWidget extends BaseWidget {

  constructor(config: SidePanelConfig = {}) {
    super({
      widgetName: config.widgetName || 'BaseSidePanelWidget',
      template: undefined,  // Inline styles - no external files
      styles: undefined,
      enableAI: false,
      enableDatabase: false,
      enableRouterEvents: true,
      enableScreenshots: false
    });
  }

  /** Panel configuration - subclasses override */
  protected abstract get panelTitle(): string;
  protected abstract get panelIcon(): string;
  protected abstract get panelSide(): SidePanelSide;

  /** Whether to show the header bar (default: true). Override to false for minimal header. */
  protected get showHeader(): boolean {
    return true;
  }

  /** Collapse button character - « for left, » for right */
  protected get collapseChar(): string {
    return this.panelSide === 'left' ? '«' : '»';
  }

  /** Panel-specific initialization */
  protected abstract onPanelInitialize(): Promise<void>;

  /** Panel-specific cleanup */
  protected abstract onPanelCleanup(): Promise<void>;

  /** Render the panel's main content (inside .panel-content) */
  protected abstract renderPanelContent(): Promise<string>;

  // === BaseWidget Implementation ===

  protected async onWidgetInitialize(): Promise<void> {
    await this.onPanelInitialize();
  }

  protected async onWidgetCleanup(): Promise<void> {
    await this.onPanelCleanup();
  }

  protected async renderWidget(): Promise<void> {
    const styles = this.getSidePanelStyles();
    const additionalStyles = this.getAdditionalStyles();
    const content = await this.renderPanelContent();

    if (this.showHeader) {
      this.shadowRoot!.innerHTML = `
        <style>${styles}${additionalStyles}</style>
        <div class="panel-header">
          <div class="panel-title">
            <span class="panel-title-icon">${this.panelIcon}</span>
            <span>${this.panelTitle}</span>
          </div>
          <button class="collapse-btn" title="Collapse panel">${this.collapseChar}</button>
        </div>
        <div class="panel-content">
          ${content}
        </div>
      `;
    } else {
      // Minimal header: just floating collapse button
      this.shadowRoot!.innerHTML = `
        <style>${styles}${additionalStyles}</style>
        <button class="collapse-btn floating" title="Collapse panel">${this.collapseChar}</button>
        <div class="panel-content full-height">
          ${content}
        </div>
      `;
    }

    this.setupCollapseButton();
    await this.onPanelRendered();
  }

  /** Override to add custom styles */
  protected getAdditionalStyles(): string {
    return '';
  }

  /** Called after render - subclasses can override to set up event listeners */
  protected async onPanelRendered(): Promise<void> {
    // Default: nothing
  }

  // === Collapse/Expand via PanelResizer (single source of truth) ===

  private setupCollapseButton(): void {
    const collapseBtn = this.shadowRoot?.querySelector('.collapse-btn');
    if (collapseBtn) {
      collapseBtn.addEventListener('click', () => {
        this.toggleCollapse();
      });
    }
  }

  protected toggleCollapse(): void {
    const continuumWidget = document.querySelector('continuum-widget') as any;
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as any;
      if (resizer?.toggle) {
        resizer.toggle();
      }
    }
  }

  protected collapse(): void {
    const continuumWidget = document.querySelector('continuum-widget') as any;
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as any;
      if (resizer?.collapse) {
        resizer.collapse();
      }
    }
  }

  protected expand(): void {
    const continuumWidget = document.querySelector('continuum-widget') as any;
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as any;
      if (resizer?.expand) {
        resizer.expand();
      }
    }
  }

  // === Common Side Panel Styles ===

  private getSidePanelStyles(): string {
    return `
      :host {
        display: block;
        position: relative;
        height: 100%;
        width: 100%;
        overflow: hidden;
        background: var(--sidebar-background, rgba(20, 25, 35, 0.95));
        color: var(--content-primary, #e0e6ed);
      }

      .panel-header {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: 35px;
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 8px 12px;
        border-bottom: 1px solid var(--border-subtle, rgba(255,255,255,0.1));
        background: rgba(0, 0, 0, 0.2);
        z-index: 5;
        box-sizing: border-box;
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

      /* Floating collapse button (no header mode) */
      .collapse-btn.floating {
        position: absolute;
        top: 8px;
        right: 8px;
        z-index: 10;
      }

      .panel-content.full-height {
        top: 0;  /* No header, start from top */
      }

      .panel-content {
        position: absolute;
        top: 35px;  /* Default: below header */
        left: 0;
        right: 0;
        bottom: 0;
        overflow: hidden;  /* Let children handle their own scrolling */
      }

      /* Scrollbar styling for panel content */
      .panel-content::-webkit-scrollbar {
        width: 6px;
      }

      .panel-content::-webkit-scrollbar-track {
        background: transparent;
      }

      .panel-content::-webkit-scrollbar-thumb {
        background: var(--border-subtle, rgba(255,255,255,0.2));
        border-radius: 3px;
      }

      .panel-content::-webkit-scrollbar-thumb:hover {
        background: var(--content-secondary, rgba(255,255,255,0.3));
      }

      /* Widget containers inside panel */
      .widget-container {
        flex: 1;
        min-height: 0;
        display: flex;
        flex-direction: column;
        overflow: hidden;
      }
    `;
  }
}
