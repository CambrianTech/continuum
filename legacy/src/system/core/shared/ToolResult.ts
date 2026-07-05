/**
 * ToolResult - Unified tool execution result event system
 *
 * Provides a standard interface for ALL tools to emit results that can be:
 * 1. Captured for persona long-term memory
 * 2. Displayed in UI
 * 3. Logged/audited
 * 4. Chained to other tools
 *
 * Usage:
 * ```typescript
 * import { ToolResult } from '@system/core/shared/ToolResult';
 *
 * // In any command execute():
 * const result = await doWork();
 * ToolResult.emit({
 *   tool: 'code/write',
 *   handle: uuid(),
 *   userId: params.userId,
 *   success: true,
 *   summary: `Wrote 150 bytes to ${filePath}`,
 *   data: { path: filePath, bytes: 150 }
 * });
 * ```
 */

import { Events } from './Events';

/**
 * Standard tool result payload - required for memory capture
 */
export interface ToolResultEvent {
  /** The tool/command that executed (e.g., 'sentinel/run', 'code/write') */
  tool: string;

  /** Unique handle for this execution */
  handle: string;

  /** The persona/user who initiated this (REQUIRED for memory capture) */
  userId?: string;

  /** Whether execution succeeded */
  success: boolean;

  /** Human-readable summary */
  summary: string;

  /** Tool-specific result data */
  data?: Record<string, unknown>;

  /** If failed, the error message */
  error?: string;

  /** Duration in milliseconds */
  durationMs?: number;

  /** Timestamp of execution */
  timestamp?: string;
}

/**
 * Tool progress event for long-running operations
 */
export interface ToolProgressEvent {
  /** The tool/command */
  tool: string;

  /** Handle for this execution */
  handle: string;

  /** Who initiated */
  userId?: string;

  /** Current step/phase */
  step: string;

  /** Progress 0-100 */
  progress: number;

  /** Status message */
  message: string;
}

/**
 * Unified tool result events
 */
export const TOOL_EVENTS = {
  /** Tool completed (success or failure) */
  RESULT: 'tool:result',

  /** Tool progress update */
  PROGRESS: 'tool:progress',

  /** Tool started */
  STARTED: 'tool:started',
} as const;

/**
 * ToolResult - Unified tool result emission
 */
export class ToolResult {
  /**
   * Emit a tool result event
   */
  static emit(event: ToolResultEvent): void {
    const fullEvent = {
      ...event,
      timestamp: event.timestamp || new Date().toISOString(),
    };

    Events.emit(TOOL_EVENTS.RESULT, fullEvent);

    // Also emit tool-specific event for backwards compatibility
    // e.g., 'sentinel/run' → 'sentinel:complete' or 'sentinel:error'
    if (event.tool.startsWith('sentinel/')) {
      const sentinelType = event.data?.type || 'unknown';
      if (event.success) {
        Events.emit('sentinel:complete', {
          handle: event.handle,
          type: sentinelType,
          userId: event.userId,
          success: true,
          data: event.data,
        });
      } else {
        Events.emit('sentinel:error', {
          handle: event.handle,
          type: sentinelType,
          userId: event.userId,
          error: event.error || event.summary,
        });
      }
    }
  }

  /**
   * Emit a progress event
   */
  static progress(event: ToolProgressEvent): void {
    Events.emit(TOOL_EVENTS.PROGRESS, event);
  }

  /**
   * Emit a started event
   */
  static started(tool: string, handle: string, userId?: string): void {
    Events.emit(TOOL_EVENTS.STARTED, {
      tool,
      handle,
      userId,
      timestamp: new Date().toISOString(),
    });
  }

  /**
   * Create a scoped emitter for a specific tool
   */
  static forTool(tool: string, handle: string, userId?: string): ScopedToolResult {
    return new ScopedToolResult(tool, handle, userId);
  }
}

/**
 * Scoped tool result emitter - convenient for use within a command
 */
export class ScopedToolResult {
  private readonly startTime = Date.now();

  constructor(
    private readonly tool: string,
    private readonly handle: string,
    private readonly userId?: string
  ) {
    ToolResult.started(tool, handle, userId);
  }

  /**
   * Emit success
   */
  success(summary: string, data?: Record<string, unknown>): void {
    ToolResult.emit({
      tool: this.tool,
      handle: this.handle,
      userId: this.userId,
      success: true,
      summary,
      data,
      durationMs: Date.now() - this.startTime,
    });
  }

  /**
   * Emit failure
   */
  failure(summary: string, error?: string, data?: Record<string, unknown>): void {
    ToolResult.emit({
      tool: this.tool,
      handle: this.handle,
      userId: this.userId,
      success: false,
      summary,
      error: error || summary,
      data,
      durationMs: Date.now() - this.startTime,
    });
  }

  /**
   * Emit progress
   */
  progress(step: string, progress: number, message: string): void {
    ToolResult.progress({
      tool: this.tool,
      handle: this.handle,
      userId: this.userId,
      step,
      progress,
      message,
    });
  }
}
