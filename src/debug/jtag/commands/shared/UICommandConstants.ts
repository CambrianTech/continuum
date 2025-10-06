/**
 * UI Command Constants
 *
 * Top-level UI interaction commands.
 * Usage:
 *   import { UI_COMMANDS } from './commands/shared/UICommandConstants';
 *   await Commands.execute(UI_COMMANDS.SCREENSHOT, params);
 */

export const UI_COMMANDS = {
  /** Capture screenshot */
  SCREENSHOT: 'screenshot',

  /** Click element */
  CLICK: 'click',

  /** Type text into element */
  TYPE: 'type',

  /** Scroll page or element */
  SCROLL: 'scroll',

  /** Navigate to URL */
  NAVIGATE: 'navigate',

  /** Navigate via proxy */
  PROXY_NAVIGATE: 'proxy-navigate',

  /** Wait for element to appear */
  WAIT_FOR_ELEMENT: 'wait-for-element',

  /** Get text from element */
  GET_TEXT: 'get-text',

  /** Show loading indicator */
  INDICATOR: 'indicator',
} as const;

/**
 * Type-safe UI command names
 */
export type UICommand = typeof UI_COMMANDS[keyof typeof UI_COMMANDS];
