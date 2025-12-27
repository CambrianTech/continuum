/**
 * LayoutManager - Manages widget layout state
 *
 * Single source of truth for which widgets appear in which panels.
 * Responds to content type changes and recipe configurations.
 *
 * Emits events when layout changes so panels can update.
 */

import { Events } from '../core/shared/Events';
import {
  type LayoutConfig,
  type LayoutWidget,
  type WidgetPosition,
  getLayoutForContentType,
  getWidgetsForPosition,
  isPanelVisible
} from './LayoutTypes';

/**
 * Event payloads for layout changes
 */
export interface LayoutChangedPayload {
  contentType: string;
  layout: LayoutConfig;
}

export interface PanelWidgetsPayload {
  position: WidgetPosition;
  widgets: LayoutWidget[];
  visible: boolean;
}

/**
 * Layout event constants
 */
export const LAYOUT_EVENTS = {
  /** Full layout changed (content type switch) */
  LAYOUT_CHANGED: 'layout:changed',

  /** Request to update a specific panel's widgets */
  PANEL_UPDATE: 'layout:panel:update',

  /** Widget position changed (drag/drop) */
  WIDGET_MOVED: 'layout:widget:moved'
} as const;

/**
 * LayoutManager - Singleton service for layout state
 */
class LayoutManagerService {
  private currentContentType: string = 'chat';
  private currentLayout: LayoutConfig;
  private _isInitialized = false;

  constructor() {
    this.currentLayout = getLayoutForContentType('chat');
  }

  /**
   * Initialize the layout manager
   * Call this early in app startup
   */
  initialize(): void {
    if (this._isInitialized) return;

    console.log('📐 LayoutManager: Initializing...');

    // Listen for content type changes from MainWidget
    Events.subscribe('content:switched', (data: { contentType: string }) => {
      this.setContentType(data.contentType);
    });

    this._isInitialized = true;
    console.log('✅ LayoutManager: Initialized');
  }

  /**
   * Get current content type
   */
  get contentType(): string {
    return this.currentContentType;
  }

  /**
   * Get current layout config
   */
  get layout(): LayoutConfig {
    return this.currentLayout;
  }

  /**
   * Set content type and update layout
   */
  setContentType(contentType: string): void {
    if (contentType === this.currentContentType) return;

    console.log(`📐 LayoutManager: Switching to ${contentType}`);

    this.currentContentType = contentType;
    this.currentLayout = getLayoutForContentType(contentType);

    // Emit layout changed event
    Events.emit(LAYOUT_EVENTS.LAYOUT_CHANGED, {
      contentType,
      layout: this.currentLayout
    } as LayoutChangedPayload);

    // Emit panel-specific updates
    this.emitPanelUpdates();
  }

  /**
   * Get widgets for a specific panel
   */
  getWidgetsForPanel(position: WidgetPosition): LayoutWidget[] {
    return getWidgetsForPosition(this.currentLayout, position);
  }

  /**
   * Check if a panel should be visible
   */
  isPanelVisible(position: WidgetPosition): boolean {
    return isPanelVisible(this.currentLayout, position);
  }

  /**
   * Move a widget to a different position (for drag/drop)
   */
  moveWidget(widgetId: string, newPosition: WidgetPosition, newOrder: number): void {
    const widget = this.currentLayout.widgets.find(w => w.id === widgetId);
    if (!widget) {
      console.warn(`📐 LayoutManager: Widget ${widgetId} not found`);
      return;
    }

    const oldPosition = widget.position;
    widget.position = newPosition;
    widget.order = newOrder;

    // Re-sort widgets in affected panels
    this.reorderPanel(oldPosition);
    if (oldPosition !== newPosition) {
      this.reorderPanel(newPosition);
    }

    // Emit events
    Events.emit(LAYOUT_EVENTS.WIDGET_MOVED, {
      widgetId,
      oldPosition,
      newPosition,
      newOrder
    });

    // Update affected panels
    this.emitPanelUpdate(oldPosition);
    if (oldPosition !== newPosition) {
      this.emitPanelUpdate(newPosition);
    }
  }

  /**
   * Subscribe to layout changes
   */
  onLayoutChanged(callback: (payload: LayoutChangedPayload) => void): () => void {
    // Events.subscribe returns an unsubscribe function
    return Events.subscribe(LAYOUT_EVENTS.LAYOUT_CHANGED, callback);
  }

  /**
   * Subscribe to panel updates
   */
  onPanelUpdate(position: WidgetPosition, callback: (payload: PanelWidgetsPayload) => void): () => void {
    const handler = (payload: PanelWidgetsPayload) => {
      if (payload.position === position) {
        callback(payload);
      }
    };
    // Events.subscribe returns an unsubscribe function
    return Events.subscribe(LAYOUT_EVENTS.PANEL_UPDATE, handler);
  }

  // === Private Methods ===

  private emitPanelUpdates(): void {
    (['left', 'center', 'right'] as WidgetPosition[]).forEach(position => {
      this.emitPanelUpdate(position);
    });
  }

  private emitPanelUpdate(position: WidgetPosition): void {
    Events.emit(LAYOUT_EVENTS.PANEL_UPDATE, {
      position,
      widgets: this.getWidgetsForPanel(position),
      visible: this.isPanelVisible(position)
    } as PanelWidgetsPayload);
  }

  private reorderPanel(position: WidgetPosition): void {
    // Get widgets for this panel and re-assign order
    const panelWidgets = this.currentLayout.widgets
      .filter(w => w.position === position)
      .sort((a, b) => a.order - b.order);

    panelWidgets.forEach((widget, index) => {
      widget.order = index;
    });
  }
}

/**
 * Singleton instance
 */
export const LayoutManager = new LayoutManagerService();
