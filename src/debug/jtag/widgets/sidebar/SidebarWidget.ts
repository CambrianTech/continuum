/**
 * SidebarWidget - Left sidebar panel with responsive content
 *
 * Shows different widgets based on content type:
 * - Default (chat): Emoter, histogram, metrics, room list, user list
 * - Settings: Settings navigation
 * - Help: Help topics navigation
 *
 * RESPONSIVE DESIGN: Persistent widgets (emoter, histogram, metrics, user-list)
 * are rendered ONCE and survive tab switches. Only the dynamic slot is swapped
 * when content type changes. This preserves animations and prevents expensive
 * widget recreation.
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
import { LAYOUT_EVENTS, type LayoutChangedPayload, type LayoutWidget, DEFAULT_LAYOUTS, getWidgetsForPosition, getLayoutForContentType, GLOBAL_LAYOUT } from '../../system/layout';
import { styles as SIDE_PANEL_STYLES } from '../shared/styles/side-panel.styles';
import { styles as SIDEBAR_STYLES } from './public/sidebar-widget.styles';

/**
 * Set of widget tag names that are globally persistent (from GLOBAL_LAYOUT).
 * These widgets survive ALL content type changes and should never be destroyed.
 * Content-specific widgets (even if marked persistent: true) are swapped on navigation.
 */
const GLOBAL_WIDGET_NAMES = new Set(
  GLOBAL_LAYOUT.widgets
    .filter(w => w.position === 'left')
    .map(w => w.widget)
);

export class SidebarWidget extends ReactiveWidget {
  // Static styles
  static override styles = [
    ReactiveWidget.styles,
    unsafeCSS(SIDE_PANEL_STYLES),
    unsafeCSS(SIDEBAR_STYLES)
  ] as CSSResultGroup;

  // Reactive state
  @reactive() private currentContentType: string = 'chat';
  @reactive() private leftWidgets: LayoutWidget[] = [];

  // Non-reactive state (internal tracking)
  private _eventUnsubscribers: Array<() => void> = [];
  private _persistentWidgetsCreated: boolean = false;
  private _currentDynamicWidgetTag: string | null = null;

  // Widget cache - key is widget tag name, value is created element
  private _widgetCache = new Map<string, HTMLElement>();

  constructor() {
    super({
      widgetName: 'SidebarWidget'
    });
  }

  // === Panel Configuration ===

  protected get panelSide(): 'left' | 'right' {
    return 'left';
  }

  protected get showHeader(): boolean {
    return false;  // Just floating « button
  }

  protected get collapseChar(): string {
    return '«';
  }

  // === Lifecycle ===

  protected override async onFirstRender(): Promise<void> {
    super.onFirstRender();
    this.verbose() && console.log('🎯 SidebarWidget: Initializing...');

    // Detect initial content type from URL
    const initialContentType = this.detectContentTypeFromUrl();
    this.verbose() && console.log(`📐 SidebarWidget: Initial content type from URL: ${initialContentType}`);
    this.updateLayout(initialContentType);

    // Listen for layout changes when content type switches
    this._eventUnsubscribers.push(
      Events.subscribe(LAYOUT_EVENTS.LAYOUT_CHANGED, (payload: LayoutChangedPayload) => {
        if (payload.contentType !== this.currentContentType) {
          this.verbose() && console.log(`📐 SidebarWidget: Layout changed to ${payload.contentType}`);
          this.updateLayout(payload.contentType);
        }
      }),
      Events.subscribe('content:switched', (data: { contentType?: string }) => {
        if (data.contentType && data.contentType !== this.currentContentType) {
          this.verbose() && console.log(`📐 SidebarWidget: Content switched to ${data.contentType}`);
          this.updateLayout(data.contentType);
        }
      })
    );
  }

  protected override onDisconnect(): void {
    super.onDisconnect();

    // Unsubscribe from ALL events to prevent memory leaks
    for (const unsub of this._eventUnsubscribers) {
      try { unsub(); } catch { /* ignore */ }
    }
    this._eventUnsubscribers = [];

    // Reset state
    this._persistentWidgetsCreated = false;
    this._currentDynamicWidgetTag = null;
    this._widgetCache.clear();

    this.verbose() && console.log('🧹 SidebarWidget: Cleanup complete');
  }

  // === URL Detection ===

  private detectContentTypeFromUrl(): string {
    const pathname = window.location.pathname;
    const firstSegment = pathname.split('/').filter(Boolean)[0] || '';
    const knownContentTypes = Object.keys(DEFAULT_LAYOUTS);
    if (knownContentTypes.includes(firstSegment)) {
      return firstSegment;
    }
    return 'chat';
  }

  // === Layout Management ===

  private updateLayout(contentType: string): void {
    this.currentContentType = contentType;
    const layout = getLayoutForContentType(contentType);
    this.leftWidgets = getWidgetsForPosition(layout, 'left');

    this.verbose() && console.log(`📐 SidebarWidget: Got ${this.leftWidgets.length} left widgets for ${contentType}:`,
      this.leftWidgets.map(w => w.widget));

    // Trigger re-render - Lit will diff and update efficiently
    this.requestUpdate();
  }

  // === Render ===

  protected override renderContent(): TemplateResult {
    // Minimal header: just floating collapse button
    const dynamicWidget = this.renderDynamicWidget();

    return html`
      <button class="collapse-btn floating" title="Collapse panel" @click=${this.handleCollapse}>
        ${this.collapseChar}
      </button>
      <div class="panel-content full-height">
        <div class="sidebar-widgets">
          ${this.renderGlobalWidgetsBefore()}
          ${dynamicWidget ? html`<div class="widget-slot widget-slot--dynamic">${dynamicWidget}</div>` : ''}
          ${this.renderGlobalWidgetsAfter()}
        </div>
      </div>
    `;
  }

  /**
   * Render global widgets that appear BEFORE the dynamic slot (order < 0)
   */
  private renderGlobalWidgetsBefore(): TemplateResult[] {
    const isGlobal = (w: LayoutWidget) => GLOBAL_WIDGET_NAMES.has(w.widget);
    const globalBefore = this.leftWidgets.filter(w => isGlobal(w) && w.order < 0);

    return globalBefore.map(w => html`
      <div class="widget-slot widget-slot--persistent">
        ${this.getOrCreateWidget(w)}
      </div>
    `);
  }

  /**
   * Render global widgets that appear AFTER the dynamic slot (order > 0)
   */
  private renderGlobalWidgetsAfter(): TemplateResult[] {
    const isGlobal = (w: LayoutWidget) => GLOBAL_WIDGET_NAMES.has(w.widget);
    const globalAfter = this.leftWidgets.filter(w => isGlobal(w) && w.order > 0);

    return globalAfter.map(w => html`
      <div class="widget-slot widget-slot--persistent">
        ${this.getOrCreateWidget(w)}
      </div>
    `);
  }

  /**
   * Render the dynamic widget (content-specific, swaps on navigation)
   */
  private renderDynamicWidget(): HTMLElement | null {
    const isGlobal = (w: LayoutWidget) => GLOBAL_WIDGET_NAMES.has(w.widget);
    const dynamicWidget = this.leftWidgets.find(w => !isGlobal(w));

    if (!dynamicWidget) return null;

    const newTag = dynamicWidget.widget;
    if (newTag !== this._currentDynamicWidgetTag) {
      this.verbose() && console.log(`📐 SidebarWidget: Dynamic widget changed: ${this._currentDynamicWidgetTag} → ${newTag}`);
      this._currentDynamicWidgetTag = newTag;
    }

    return this.getOrCreateWidget(dynamicWidget);
  }

  /**
   * Get a cached widget or create a new one
   * Widgets are cached to preserve state across re-renders
   */
  private getOrCreateWidget(layoutWidget: LayoutWidget): HTMLElement {
    const tagName = layoutWidget.widget;
    let widget = this._widgetCache.get(tagName);

    if (!widget) {
      widget = document.createElement(tagName);

      // Apply config as attributes
      if (layoutWidget.config) {
        for (const [key, value] of Object.entries(layoutWidget.config)) {
          if (typeof value === 'string') {
            widget.setAttribute(key, value);
          } else if (typeof value === 'boolean' && value) {
            widget.setAttribute(key, '');
          } else if (typeof value === 'number') {
            widget.setAttribute(key, String(value));
          }
        }
      }

      this._widgetCache.set(tagName, widget);
      this.verbose() && console.log(`📐 SidebarWidget: Created and cached ${tagName}`);
    }

    return widget;
  }

  // === Event Handlers ===

  private handleCollapse = (): void => {
    const continuumWidget = document.querySelector('continuum-widget');
    if (continuumWidget?.shadowRoot) {
      const resizer = continuumWidget.shadowRoot.querySelector(
        `panel-resizer[side="${this.panelSide}"]`
      ) as any;
      resizer?.toggle?.();
    }
  };
}

// Registration handled by centralized BROWSER_WIDGETS registry
