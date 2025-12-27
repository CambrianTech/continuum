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

  /** If true, this widget persists across all content types (from global layout) */
  persistent?: boolean;
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
 * Global layout - persistent widgets that appear across ALL content types
 * These are merged with content-specific layouts (global first, then content-specific)
 *
 * Persistent widgets:
 * - continuum-emoter-widget: HAL 9000 system status indicator
 * - cognition-histogram-widget: AI pipeline visualization
 * - continuum-metrics-widget: AI performance dashboard
 */
export const GLOBAL_LAYOUT: LayoutConfig = {
  widgets: [
    // Left panel persistent widgets (always show at top of sidebar)
    { widget: 'continuum-emoter-widget', position: 'left', order: -100, persistent: true },
    { widget: 'cognition-histogram-widget', position: 'left', order: -90, persistent: true },
    { widget: 'continuum-metrics-widget', position: 'left', order: -80, persistent: true }
  ]
};

/**
 * Default layouts for content types
 * These are MERGED with GLOBAL_LAYOUT - persistent widgets always appear
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

  // Settings layout - settings nav left (persistent), settings form center, help assistant right
  'settings': {
    widgets: [
      { widget: 'settings-nav-widget', position: 'left', order: 0, persistent: true },
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

  // Logs layout - logs nav left, log viewer center, AI assistant right for debugging help
  'logs': {
    widgets: [
      { widget: 'logs-nav-widget', position: 'left', order: 0, persistent: true },
      { widget: 'log-viewer-widget', position: 'center', order: 0 },
      { widget: 'chat-widget', position: 'right', order: 0, config: { room: 'help', compact: true } }
    ]
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
 * Get layout for a content type, merging global persistent widgets
 *
 * Inheritance model:
 * 1. Start with GLOBAL_LAYOUT (persistent widgets)
 * 2. Merge content-specific layout on top
 * 3. Sort by order (negative orders appear first)
 *
 * This ensures emoter, histogram, metrics always appear in sidebar
 * while content-specific widgets (room-list, settings-nav) appear below them.
 */
export function getLayoutForContentType(contentType: string): LayoutConfig {
  const contentLayout = DEFAULT_LAYOUTS[contentType] || DEFAULT_LAYOUTS['chat'];

  // Merge global persistent widgets with content-specific widgets
  const mergedWidgets = [
    ...GLOBAL_LAYOUT.widgets,
    ...contentLayout.widgets
  ];

  return {
    widgets: mergedWidgets,
    panels: contentLayout.panels
  };
}

/**
 * Parse a recipe layout (supports both old and new formats)
 *
 * Old format:
 * ```json
 * { "main": ["settings-widget"], "right": { "widgets": ["chat-widget"], "config": {...} } }
 * ```
 *
 * New format:
 * ```json
 * { "widgets": [ { "widget": "settings-nav-widget", "position": "left", "order": 0 } ] }
 * ```
 */
export function parseRecipeLayout(recipeLayout: unknown): LayoutConfig | null {
  if (!recipeLayout || typeof recipeLayout !== 'object') {
    return null;
  }

  const layout = recipeLayout as Record<string, unknown>;

  // New format: has 'widgets' array with position/order
  if (Array.isArray(layout.widgets)) {
    const widgets: LayoutWidget[] = (layout.widgets as unknown[])
      .filter((w): w is Record<string, unknown> => !!w && typeof w === 'object')
      .map((w, index) => ({
        widget: String(w.widget || 'unknown-widget'),
        position: (w.position as WidgetPosition) || 'center',
        order: typeof w.order === 'number' ? w.order : index,
        config: w.config as Record<string, unknown> | undefined,
        id: w.id as string | undefined
      }));

    const panelConfig = layout.panels as Record<string, { visible?: boolean }> | undefined;

    return {
      widgets,
      panels: panelConfig ? {
        left: panelConfig.left?.visible !== undefined ? { visible: panelConfig.left.visible } : undefined,
        center: panelConfig.center?.visible !== undefined ? { visible: panelConfig.center.visible } : undefined,
        right: panelConfig.right?.visible !== undefined ? { visible: panelConfig.right.visible } : undefined
      } : undefined
    };
  }

  // Old format: has 'main' and/or 'right' arrays
  if (layout.main || layout.right || layout.left) {
    const widgets: LayoutWidget[] = [];

    // Parse left panel
    if (Array.isArray(layout.left)) {
      (layout.left as string[]).forEach((widget, index) => {
        widgets.push({ widget, position: 'left', order: index });
      });
    }

    // Parse main/center panel
    if (Array.isArray(layout.main)) {
      (layout.main as string[]).forEach((widget, index) => {
        widgets.push({ widget, position: 'center', order: index });
      });
    }

    // Parse right panel (can be array or object with widgets + config)
    if (layout.right) {
      if (Array.isArray(layout.right)) {
        (layout.right as string[]).forEach((widget, index) => {
          widgets.push({ widget, position: 'right', order: index });
        });
      } else if (typeof layout.right === 'object' && layout.right !== null) {
        const rightConfig = layout.right as Record<string, unknown>;
        if (Array.isArray(rightConfig.widgets)) {
          (rightConfig.widgets as string[]).forEach((widget, index) => {
            widgets.push({
              widget,
              position: 'right',
              order: index,
              config: rightConfig.config as Record<string, unknown> | undefined
            });
          });
        }
      }
    }

    return { widgets };
  }

  return null;
}
