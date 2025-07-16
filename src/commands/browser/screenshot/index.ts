// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Screenshot Module - Middle-out Architecture Export
 * 
 * Self-contained module for screenshot functionality following the
 * shared/client/server pattern
 */

// Shared types and base classes
export type { 
  ScreenshotParams,
  ScreenshotClientRequest,
  ScreenshotResult
} from './shared/ScreenshotTypes';
export { 
  ScreenshotFormat,
  ScreenshotDestination,
  ScreenshotAnimation
} from './shared/ScreenshotTypes';
export type {
  ScreenshotBaseOptions,
  ScreenshotCaptureResult
} from './shared/ScreenshotBase';
export { ScreenshotBase } from './shared/ScreenshotBase';

// Client exports
export { ScreenshotClient, clientScreenshot } from './client/ScreenshotClient';

// Server exports
export { ScreenshotCommand } from './server/ScreenshotCommand';

// Backward compatibility (deprecated)
export { clientScreenshot as legacyClientScreenshot } from './client/ScreenshotClient';