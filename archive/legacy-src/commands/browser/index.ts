/**
 * Browser Commands
 * 
 * Entry point for browser command category.
 * This file should export all commands in this category.
 */

// Screenshot command module
export * from './screenshot';

// Other browser commands
export { BrowserCommand } from './browser/BrowserCommand';
export { BrowserConsoleCommand } from './console/BrowserConsoleCommand';
export { JSExecuteCommand } from './js-execute/JSExecuteCommand';
export { PromisejsCommand } from './promisejs/PromisejsCommand';
export { WidgetInspectCommand } from './widget-inspect/WidgetInspectCommand';
