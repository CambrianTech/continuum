/**
 * Debug Command: widget-css
 * Hot-inject CSS into widgets for rapid iteration without full deployment
 */

import type { CommandParams, CommandResult } from '@system/core/types/JTAGTypes';

export type CSSInjectionMode = 'append' | 'replace' | 'replaceAll' | 'debugOnly';

export interface WidgetCSSDebugParams extends CommandParams {
  widgetSelector: string;           // e.g., "user-list-widget"
  cssContent?: string;               // CSS to inject (inline)
  cssFile?: string;                  // Path to CSS file to inject
  screenshot?: boolean;              // Take screenshot after injection (default: true)
  filename?: string;                 // Screenshot filename (default: "widget-css-debug.png")

  // Extract mode - pull CSS out of widget
  extract?: boolean;                 // Extract current CSS from widget (returns as text for redirect)

  // Injection mode (default: 'replace')
  mode?: CSSInjectionMode;           // 'append': add to existing | 'replace': replace debug styles only | 'replaceAll': clear ALL styles | 'debugOnly': only add debug overlays
  clearExisting?: boolean;           // DEPRECATED: use mode='replaceAll' instead

  // Advanced features
  showBoundingBoxes?: boolean;       // Overlay bounding boxes for layout debugging
  highlightFlexboxes?: boolean;      // Highlight flex containers
  animateChanges?: boolean;          // Animate CSS changes for visual feedback (default: false)
  multiWidget?: boolean;             // Apply to all widgets matching selector (default: false)
}

export interface WidgetCSSDebugResult extends CommandResult {
  success: boolean;
  widgetSelector: string;
  cssInjected: boolean;
  widgetsAffected?: number;          // Count if multiWidget=true
  screenshotTaken?: boolean;
  screenshotPath?: string;
  cssVariables?: Record<string, string>;  // Available CSS variables
  extractedCSS?: string;             // Extracted CSS when extract=true
  error?: string;
}

export function createWidgetCSSDebugResult(
  context: CommandParams['context'],
  sessionId: CommandParams['sessionId'],
  data: Omit<WidgetCSSDebugResult, 'context' | 'sessionId'>
): WidgetCSSDebugResult {
  return {
    context,
    sessionId,
    ...data
  };
}
