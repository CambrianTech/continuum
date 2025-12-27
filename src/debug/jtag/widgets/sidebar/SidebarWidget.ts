/**
 * SidebarWidget - Left sidebar panel with dynamic content
 *
 * Shows different widgets based on content type:
 * - Default (chat): Emoter, histogram, metrics, room list, user list
 * - Settings: Settings navigation
 * - Help: Help topics navigation
 *
 * Uses LayoutManager to determine which widgets to show.
 * Extends BaseSidePanelWidget for consistent panel behavior.
 */

import { BaseSidePanelWidget, type SidePanelSide } from '../shared/BaseSidePanelWidget';
import { Events } from '../../system/core/shared/Events';
import { LAYOUT_EVENTS, type LayoutChangedPayload, type LayoutWidget, DEFAULT_LAYOUTS, getWidgetsForPosition, getLayoutForContentType } from '../../system/layout';

export class SidebarWidget extends BaseSidePanelWidget {
  private currentContentType: string = 'chat';
  private leftWidgets: LayoutWidget[] = [];

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
    console.log('🎯 SidebarWidget: Initializing...');

    // Detect initial content type from URL
    const initialContentType = this.detectContentTypeFromUrl();
    console.log(`📐 SidebarWidget: Initial content type from URL: ${initialContentType}`);
    this.updateLayout(initialContentType);

    // Listen for layout changes when content type switches
    Events.subscribe(LAYOUT_EVENTS.LAYOUT_CHANGED, (payload: LayoutChangedPayload) => {
      console.log(`📐 SidebarWidget: Layout changed to ${payload.contentType}`);
      this.updateLayout(payload.contentType);
    });

    // Also listen for content:switched events as backup
    Events.subscribe('content:switched', (data: { contentType?: string }) => {
      if (data.contentType && data.contentType !== this.currentContentType) {
        console.log(`📐 SidebarWidget: Content switched to ${data.contentType}`);
        this.updateLayout(data.contentType);
      }
    });
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

    console.log(`📐 SidebarWidget: Got ${this.leftWidgets.length} left widgets for ${contentType}:`,
      this.leftWidgets.map(w => w.widget));

    // Re-render with new widgets - call the inherited rendering method
    this.renderPanelContent().then(html => {
      const contentContainer = this.shadowRoot?.querySelector('.panel-content');
      if (contentContainer) {
        contentContainer.innerHTML = html;
      }
    });
  }

  protected async onPanelCleanup(): Promise<void> {
    console.log('🧹 SidebarWidget: Cleanup complete');
  }

  // === Content Rendering ===

  protected async renderPanelContent(): Promise<string> {
    // Always use layout system - GLOBAL_LAYOUT provides persistent widgets,
    // content-specific layouts are merged on top by getLayoutForContentType()
    const widgetsHtml = this.leftWidgets.map(w => this.renderLayoutWidget(w)).join('\n');
    return `<div class="sidebar-widgets">${widgetsHtml}</div>`;
  }

  private renderLayoutWidget(layoutWidget: LayoutWidget): string {
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

    // Add wrapper with appropriate class for persistent vs dynamic widgets
    const wrapperClass = layoutWidget.persistent ? 'widget-slot widget-slot--persistent' : 'widget-slot widget-slot--dynamic';
    return `<div class="${wrapperClass}"><${tagName}${attrs}></${tagName}></div>`;
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
    console.log('✅ SidebarWidget: Rendered');
  }
}

// Registration handled by centralized BROWSER_WIDGETS registry
