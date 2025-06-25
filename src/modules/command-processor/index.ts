/**
 * Command Processor Module - Main Exports
 * Clean module interface with everything you need
 */

export { CommandProcessor } from './CommandProcessor';
export * from './types';

// Singleton instance ready to use
import { CommandProcessor } from './CommandProcessor';

export const commandProcessor = new CommandProcessor({
  enableCaseInsensitive: true,
  enableTypeScriptOnly: false,
  logLevel: 'info'
});

// Re-export for convenience
export default CommandProcessor;