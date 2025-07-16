/**
 * Screenshot Command - Root Level Export
 * 
 * This file exists to satisfy the command discovery system which expects
 * the main command file at the root level of the command directory.
 * 
 * The actual implementation is in server/ScreenshotCommand.ts following
 * the middle-out architecture pattern.
 */

// Re-export the server implementation
export { ScreenshotCommand } from './server/ScreenshotCommand';
export { ScreenshotCommand as default } from './server/ScreenshotCommand';