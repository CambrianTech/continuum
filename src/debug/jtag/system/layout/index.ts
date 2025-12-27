/**
 * Layout Module - Widget position and layout management
 *
 * Exports the unified widget array architecture where each widget
 * has a position enum enabling drag/drop between panels.
 */

export {
  type WidgetPosition,
  type LayoutWidget,
  type LayoutConfig,
  DEFAULT_LAYOUTS,
  getWidgetsForPosition,
  isPanelVisible,
  getLayoutForContentType
} from './LayoutTypes';

export {
  LayoutManager,
  LAYOUT_EVENTS,
  type LayoutChangedPayload,
  type PanelWidgetsPayload
} from './LayoutManager';
