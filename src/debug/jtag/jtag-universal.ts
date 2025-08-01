/**
 * JTAG Universal Command Bus - Main Entry Point
 * 
 * This is the main entry point for the JTAG Universal Command Bus.
 * Provides the clean API: let jtag = await JTAGSystem.connect()
 * 
 * Usage:
 *   import { JTAGSystem } from '@continuum/jtag/jtag-universal';
 *   
 *   // Auto-wire the complete system
 *   let jtag = await JTAGSystem.connect();
 *   
 *   // Use from any environment (server example)
 *   let params = { filename: "screenshot.png" };
 *   let screenshot = await jtag.commands.screenshot(params);
 */

// Export the complete Universal Command Bus
export { JTAGSystem } from './system/core/system/shared/JTAGSystem';

// Export core types that users need
export type { 
  JTAGContext, 
  JTAGEnvironment,
  JTAGPayload,
  CommandParams
} from './system/core/types/JTAGTypes';

// Export screenshot-specific types
export type {
  ScreenshotParams,
  ScreenshotResult,
  ScreenshotOptions
} from './commands/screenshot/shared/ScreenshotTypes';

// Export the base classes for extending
export { JTAGModule } from './system/core/shared/JTAGModule';
export { CommandBase } from './daemons/command-daemon/shared/CommandBase';

console.log('🚀 JTAG Universal Command Bus loaded - Use await JTAGSystem.connect()');