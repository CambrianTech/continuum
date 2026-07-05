/**
 * ScreenshotCommand - Clean Static Interface
 *
 * Provides elegant command execution from either browser or server:
 *
 * ```typescript
 * const result = await ScreenshotCommand.execute({
 *   querySelector: 'body',
 *   filename: 'test.png'
 * });
 * ```
 *
 * Seamless marshaling - works identically everywhere, properly typed.
 */

import { Commands } from '@system/core/shared/Commands';
import type { ScreenshotParams, ScreenshotResult } from './ScreenshotTypes';
import { createScreenshotParams } from './ScreenshotTypes';

import { Screenshot } from './ScreenshotTypes';
export class ScreenshotCommand {
  /**
   * Execute screenshot command with clean typing
   *
   * Works in both browser and server - marshaling handled automatically
   */
  static async execute(
    params: Omit<ScreenshotParams, 'context' | 'sessionId'>
  ): Promise<ScreenshotResult> {
    return await Screenshot.execute(params
    );
  }

  /**
   * Type-safe parameter builder (optional convenience)
   */
  static params(
    params: Omit<ScreenshotParams, 'context' | 'sessionId'>
  ): Omit<ScreenshotParams, 'context' | 'sessionId'> {
    return params;
  }
}