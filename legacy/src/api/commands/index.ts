/**
 * Commands API - Public Interface
 * 
 * Barrel export for all command parameter and result types.
 * External consumers import command types from this single entry point.
 */

// File operations
export * from './file/FileCommands';

// Chat operations  
export * from './chat/ChatCommands';

// Screenshot operations
export * from './screenshot/ScreenshotCommands';

// System operations
export * from './system/SystemCommands';

// Union types for all commands
import type { FileCommandParams, FileCommandResult } from './file/FileCommands';
import type { ChatCommandParams, ChatCommandResult } from './chat/ChatCommands';
import type { ScreenshotCommandParams, ScreenshotCommandResult } from './screenshot/ScreenshotCommands';
import type { SystemCommandParams, SystemCommandResult } from './system/SystemCommands';

// All command parameters - discriminated union
export type CommandParams = 
  | FileCommandParams
  | ChatCommandParams  
  | ScreenshotCommandParams
  | SystemCommandParams;

// All command results - discriminated union
export type CommandResult = 
  | FileCommandResult
  | ChatCommandResult
  | ScreenshotCommandResult
  | SystemCommandResult;

// Command categories for organization
export const COMMAND_CATEGORIES = {
  file: 'File Operations',
  chat: 'Chat & Messaging',
  screenshot: 'Screen Capture',  
  system: 'System Management'
} as const;

export type CommandCategory = keyof typeof COMMAND_CATEGORIES;