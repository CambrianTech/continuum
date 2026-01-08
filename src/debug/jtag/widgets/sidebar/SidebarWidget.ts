/**
 * SidebarWidget - Left sidebar panel with responsive content
 *
 * Shows different widgets based on content type:
 * - Default (chat): Emoter, histogram, metrics, room list, user list
 * - Settings: Settings navigation
 * - Help: Help topics navigation
 *
 * RESPONSIVE DESIGN: Persistent widgets (emoter, histogram, metrics, user-list)
 * are rendered ONCE and survive tab switches. Only the dynamic slot (room-list,
 * settings-nav, logs-nav) is swapped when content type changes. This preserves
 * animations and prevents expensive widget recreation.
 *
 * Uses LayoutManager to determine which widgets to show.
 * Extends BaseSidePanelWidget for consistent panel behavior.
 */

import { BaseSidePanelWidget, type SidePanelSide } from '../shared/BaseSidePanelWidget';
import { Events } from '../../system/core/shared/Events';
import { LAYOUT_EVENTS, type LayoutChangedPayload, type LayoutWidget, DEFAULT_LAYOUTS, getWidgetsForPosition, getLayoutForContentType, GLOBAL_LAYOUT } from '../../system/layout';

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

export class SidebarWidget extends BaseSidePanelWidget {
  private currentContentType: string = 'chat';
  private leftWidgets: LayoutWidget[] = [];
  private _eventUnsubscribers: Array<() => void> = [];

  // Responsive design state - prevent full redraws
  private _persistentWidgetsRendered: boolean = false;
  private _currentDynamicWidgetTag: string | null = null;
  private _dynamicSlot: HTMLElement | null = null;

  constructor() {
    super({
      widgetName: 'SidebarWidget'
    });
  }

  // === Panel Configuration ===

  protected get panelTitle(): string {
    return '';  // Not used - no header
  }

  protected get panelIcon(): string {
    return '';  // Not used - no header
  }

  protected get panelSide(): SidePanelSide {
    return 'left';
  }

  protected get showHeader(): boolean {
    return false;  // Just floating « button
  }

  // === Lifecycle ===

  protected async onPanelInitialize(): Promise<void> {
    this.verbose() && console.log('🎯 SidebarWidget: Initializing...');

    // Detect initial content type from URL
    const initialContentType = this.detectContentTypeFromUrl();
    this.verbose() && console.log(`📐 SidebarWidget: Initial content type from URL: ${initialContentType}`);
    this.updateLayout(initialContentType);

    // Listen for layout changes when content type switches
    this._eventUnsubscribers.push(
      Events.subscribe(LAYOUT_EVENTS.LAYOUT_CHANGED, (payload: LayoutChangedPayload) => {
        // Guard: only update if content type actually changed (prevents unnecessary DOM clearing)
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

  /**
   * Detect content type from current URL pathname
   * Maps paths like /settings, /theme, /help to their content types
   * Uses DEFAULT_LAYOUTS keys as source of truth for valid content types
   */
  private detectContentTypeFromUrl(): string {
    const pathname = window.location.pathname;

    // Get first segment from path (e.g., /settings -> 'settings')
    const firstSegment = pathname.split('/').filter(Boolean)[0] || '';

    // Check if this path matches a known layout content type
    const knownContentTypes = Object.keys(DEFAULT_LAYOUTS);
    if (knownContentTypes.includes(firstSegment)) {
      return firstSegment;
    }

    // Default to chat for root or unknown paths
    return 'chat';
  }

  private updateLayout(contentType: string): void {
    this.currentContentType = contentType;
    const layout = getLayoutForContentType(contentType);
    this.leftWidgets = getWidgetsForPosition(layout, 'left');

    this.verbose() && console.log(`📐 SidebarWidget: Got ${this.leftWidgets.length} left widgets for ${contentType}:`,
      this.leftWidgets.map(w => w.widget));

    // RESPONSIVE DESIGN: Only update the dynamic slot, not globally persistent widgets
    if (this._persistentWidgetsRendered && this._dynamicSlot) {
      // Find the dynamic widget for this content type (not in GLOBAL_LAYOUT)
      const dynamicWidget = this.leftWidgets.find(w => !GLOBAL_WIDGET_NAMES.has(w.widget));
      const newTag = dynamicWidget?.widget || null;

      // Only update if dynamic widget actually changed
      if (newTag !== this._currentDynamicWidgetTag) {
        this.verbose() && console.log(`📐 SidebarWidget: Swapping dynamic widget: ${this._currentDynamicWidgetTag} → ${newTag}`);
        this._currentDynamicWidgetTag = newTag;

        // Clear and recreate only the dynamic slot content
        this._dynamicSlot.innerHTML = '';
        if (dynamicWidget) {
          const widgetEl = this.createWidgetElement(dynamicWidget);
          this._dynamicSlot.appendChild(widgetEl);
        }
      } else {
        this.verbose() && console.log(`📐 SidebarWidget: Dynamic widget unchanged (${newTag}), skipping redraw`);
      }
    } else {
      // First render - do full render and cache persistent state
      this.renderPanelContent().then(html => {
        const contentContainer = this.shadowRoot?.querySelector('.panel-content');
        if (contentContainer) {
          contentContainer.innerHTML = html;
          // Cache the dynamic slot reference for future updates
          this._dynamicSlot = contentContainer.querySelector('.widget-slot--dynamic') as HTMLElement;
          this._persistentWidgetsRendered = true;
          // Track current dynamic widget (not in GLOBAL_LAYOUT)
          const dynamicWidget = this.leftWidgets.find(w => !GLOBAL_WIDGET_NAMES.has(w.widget));
          this._currentDynamicWidgetTag = dynamicWidget?.widget || null;
        }
      });
    }
  }

  /**
   * Create a DOM element for a layout widget
   */
  private createWidgetElement(layoutWidget: LayoutWidget): HTMLElement {
    const el = document.createElement(layoutWidget.widget);

    // Apply config as attributes
    if (layoutWidget.config) {
      for (const [key, value] of Object.entries(layoutWidget.config)) {
        if (typeof value === 'string') {
          el.setAttribute(key, value);
        } else if (typeof value === 'boolean' && value) {
          el.setAttribute(key, '');
        } else if (typeof value === 'number') {
          el.setAttribute(key, String(value));
        }
      }
    }

    return el;
  }

  protected async onPanelCleanup(): Promise<void> {
    // Unsubscribe from ALL events to prevent memory leaks
    for (const unsub of this._eventUnsubscribers) {
      try { unsub(); } catch { /* ignore */ }
    }
    this._eventUnsubscribers = [];

    // Reset responsive state
    this._persistentWidgetsRendered = false;
    this._currentDynamicWidgetTag = null;
    this._dynamicSlot = null;

    this.verbose() && console.log('🧹 SidebarWidget: Cleanup complete');
  }

  // === Content Rendering ===

  /**
   * Render sidebar content with global persistent and dynamic slots
   *
   * Structure:
   * - Global widgets BEFORE dynamic slot (GLOBAL_LAYOUT, order < 0)
   * - Dynamic slot (content-specific widget, order ~0)
   * - Global widgets AFTER dynamic slot (GLOBAL_LAYOUT, order > 0)
   *
   * Note: We use GLOBAL_WIDGET_NAMES to identify truly global widgets,
   * not the `persistent` flag (which has different meaning in content layouts).
   */
  protected async renderPanelContent(): Promise<string> {
    // Helper to check if widget is globally persistent
    const isGlobal = (w: LayoutWidget) => GLOBAL_WIDGET_NAMES.has(w.widget);

    // Separate global widgets by order relative to dynamic slot (order 0)
    const globalBefore = this.leftWidgets.filter(w => isGlobal(w) && w.order < 0);
    const dynamicWidget = this.leftWidgets.find(w => !isGlobal(w));
    const globalAfter = this.leftWidgets.filter(w => isGlobal(w) && w.order > 0);

    // Build HTML with clear structure
    const beforeHtml = globalBefore.map(w => this.renderLayoutWidget(w, true)).join('\n');
    const dynamicHtml = dynamicWidget
      ? `<div class="widget-slot widget-slot--dynamic">${this.renderLayoutWidget(dynamicWidget, false)}</div>`
      : '<div class="widget-slot widget-slot--dynamic"></div>';
    const afterHtml = globalAfter.map(w => this.renderLayoutWidget(w, true)).join('\n');

    return `<div class="sidebar-widgets">${beforeHtml}${dynamicHtml}${afterHtml}</div>`;
  }

  /**
   * Render a single widget element (wrapped or unwrapped)
   */
  private renderLayoutWidget(layoutWidget: LayoutWidget, wrapPersistent: boolean): string {
    const tagName = layoutWidget.widget;

    // Build attributes from config
    let attrs = '';
    if (layoutWidget.config) {
      for (const [key, value] of Object.entries(layoutWidget.config)) {
        if (typeof value === 'string') {
          attrs += ` ${key}="${value}"`;
        } else if (typeof value === 'boolean' && value) {
          attrs += ` ${key}`;
        } else if (typeof value === 'number') {
          attrs += ` ${key}="${value}"`;
        }
      }
    }

    const widgetHtml = `<${tagName}${attrs}></${tagName}>`;

    // Persistent widgets get their own wrapper div for styling
    if (wrapPersistent) {
      return `<div class="widget-slot widget-slot--persistent">${widgetHtml}</div>`;
    }

    return widgetHtml;
  }

  protected getAdditionalStyles(): string {
    return `
      .sidebar-widgets {
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        display: flex;
        flex-direction: column;
        gap: var(--spacing-md, 12px);
        padding: var(--spacing-md, 12px);
        overflow-y: auto;
        overflow-x: hidden;
      }

      /* Persistent widgets - natural height, no flex grow */
      .widget-slot--persistent {
        flex-shrink: 0;
      }

      /* Dynamic widgets - flex to fill remaining space */
      .widget-slot--dynamic {
        flex: 1;
        min-height: 100px;
        display: flex;
        flex-direction: column;
        overflow: hidden;
      }

      /* Child widgets fill their container */
      .widget-slot--dynamic > * {
        flex: 1;
        min-height: 0;
      }
    `;
  }

  protected async onPanelRendered(): Promise<void> {
    this.verbose() && console.log('✅ SidebarWidget: Rendered');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
