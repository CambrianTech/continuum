/**
 * LayoutTypes - Widget position and layout system
 *
 * Unified widget array architecture where each widget has a position enum.
 * This enables drag/drop between panels by just changing the position property.
 *
 * Vision: Instead of separate left/center/right arrays in recipes,
 * we have ONE array of widgets, each with position and order.
 */

/**
 * Panel position for widgets
 * - 'left': Left sidebar (e.g., room list, settings nav)
 * - 'center': Main content area (e.g., chat, settings form)
 * - 'right': Right panel (e.g., assistant, logs)
 */
export type WidgetPosition = 'left' | 'center' | 'right';

/**
 * A widget instance in a layout
 * Combines what widget to show, where to show it, and configuration
 */
export interface LayoutWidget {
  /** Widget tag name (e.g., 'room-list-widget', 'chat-widget') */
  widget: string;

  /** Panel position */
  position: WidgetPosition;

  /** Sort order within the panel (lower = higher/first) */
  order: number;

  /** Widget-specific configuration */
  config?: Record<string, unknown>;

  /** Optional: unique ID for tracking/drag-drop (auto-generated if not provided) */
  id?: string;
}

/**
 * Layout configuration for a content type or recipe
 * Uses unified widget array instead of separate panel definitions
 */
export interface LayoutConfig {
  /** All widgets in this layout with their positions */
  widgets: LayoutWidget[];

  /** Panel visibility overrides (hide entire panels) */
  panels?: {
    left?: { visible: boolean };
    center?: { visible: boolean };
    right?: { visible: boolean };
  };
}

/**
 * Default layouts for content types
 * These are used when no recipe-specific layout is defined
 */
export const DEFAULT_LAYOUTS: Record<string, LayoutConfig> = {
  // Chat layout - room list left, chat center, assistant right
  'chat': {
    widgets: [
      { widget: 'room-list-widget', position: 'left', order: 0 },
      { widget: 'user-list-widget', position: 'left', order: 1 },
      { widget: 'chat-widget', position: 'center', order: 0 },
      { widget: 'chat-widget', position: 'right', order: 0, config: { room: 'help', compact: true } }
    ]
  },

  // Settings layout - settings nav left, settings form center, help assistant right
  'settings': {
    widgets: [
      { widget: 'settings-nav-widget', position: 'left', order: 0 },
      { widget: 'settings-widget', position: 'center', order: 0 },
      { widget: 'chat-widget', position: 'right', order: 0, config: { room: 'help', compact: true } }
    ]
  },

  // Help layout - help center, no right panel (has embedded assistant)
  'help': {
    widgets: [
      { widget: 'room-list-widget', position: 'left', order: 0 },
      { widget: 'help-widget', position: 'center', order: 0 }
    ],
    panels: {
      right: { visible: false }
    }
  },

  // Theme layout - theme picker center, assistant right
  'theme': {
    widgets: [
      { widget: 'room-list-widget', position: 'left', order: 0 },
      { widget: 'theme-widget', position: 'center', order: 0 },
      { widget: 'chat-widget', position: 'right', order: 0, config: { room: 'theme', compact: true } }
    ]
  },

  // Logs layout - logs viewer center, no right panel
  'logs': {
    widgets: [
      { widget: 'room-list-widget', position: 'left', order: 0 },
      { widget: 'logs-widget', position: 'center', order: 0 }
    ],
    panels: {
      right: { visible: false }
    }
  }
};

/**
 * Get widgets for a specific panel from a layout config
 */
export function getWidgetsForPosition(layout: LayoutConfig, position: WidgetPosition): LayoutWidget[] {
  return layout.widgets
    .filter(w => w.position === position)
    .sort((a, b) => a.order - b.order);
}

/**
 * Check if a panel should be visible
 */
export function isPanelVisible(layout: LayoutConfig, position: WidgetPosition): boolean {
  // Default to visible unless explicitly hidden
  return layout.panels?.[position]?.visible !== false;
}

/**
 * Get layout for a content type, falling back to defaults
 */
export function getLayoutForContentType(contentType: string): LayoutConfig {
  return DEFAULT_LAYOUTS[contentType] || DEFAULT_LAYOUTS['chat'];
}
