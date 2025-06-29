/**
 * Command Processor Module - Main Exports
 * Clean module interface with everything you need
 */

export { TypeScriptCommandProcessor as CommandProcessor } from './CommandProcessor';
export * from './types';

// Singleton instance ready to use
import { TypeScriptCommandProcessor } from './CommandProcessor';

export const commandProcessor = new TypeScriptCommandProcessor({
  enableCaseInsensitive: true,
  enableTypeScriptOnly: false,
  logLevel: 'info'
});

// Re-export for convenience
export default TypeScriptCommandProcessor;