/**
 * Shared Widget Styles - Index
 *
 * Exports all shared style modules for panel-type widgets.
 * Import from this file for convenience:
 *
 * ```typescript
 * import { PANEL_STYLES, FORM_STYLES, ALL_PANEL_STYLES } from '../shared/styles';
 * ```
 */

export * from './PanelStyles';
export * from './FormStyles';

// Import for combined export
import { PANEL_STYLES } from './PanelStyles';
import { FORM_STYLES } from './FormStyles';

/**
 * All panel styles combined - use this for full styling in a panel widget
 */
export const ALL_PANEL_STYLES = `
  ${PANEL_STYLES}
  ${FORM_STYLES}
`;
