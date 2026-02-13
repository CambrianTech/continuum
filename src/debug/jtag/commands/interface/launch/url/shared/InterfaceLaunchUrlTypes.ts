/**
 * Interface Launch Url Command - Shared Types
 *
 * Opens a URL in the default browser. Enables personas to view what they build.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Interface Launch Url Command Parameters
 */
export interface InterfaceLaunchUrlParams extends CommandParams {
  // URL to open in the browser
  url: string;
  // Wait for page to load before returning (default: false)
  waitForLoad?: boolean;
  // Take screenshot after loading (default: false)
  screenshot?: boolean;
}

/**
 * Factory function for creating InterfaceLaunchUrlParams
 */
export const createInterfaceLaunchUrlParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // URL to open in the browser
    url: string;
    // Wait for page to load before returning (default: false)
    waitForLoad?: boolean;
    // Take screenshot after loading (default: false)
    screenshot?: boolean;
  }
): InterfaceLaunchUrlParams => createPayload(context, sessionId, {
  waitForLoad: data.waitForLoad ?? false,
  screenshot: data.screenshot ?? false,
  ...data
});

/**
 * Interface Launch Url Command Result
 */
export interface InterfaceLaunchUrlResult extends CommandResult {
  success: boolean;
  // The URL that was opened
  url: string;
  // Whether the browser was launched successfully
  launched: boolean;
  // Path to screenshot if requested
  screenshotPath: string;
  error?: JTAGError;
}

/**
 * Factory function for creating InterfaceLaunchUrlResult with defaults
 */
export const createInterfaceLaunchUrlResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // The URL that was opened
    url?: string;
    // Whether the browser was launched successfully
    launched?: boolean;
    // Path to screenshot if requested
    screenshotPath?: string;
    error?: JTAGError;
  }
): InterfaceLaunchUrlResult => createPayload(context, sessionId, {
  url: data.url ?? '',
  launched: data.launched ?? false,
  screenshotPath: data.screenshotPath ?? '',
  ...data
});

/**
 * Smart Interface Launch Url-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createInterfaceLaunchUrlResultFromParams = (
  params: InterfaceLaunchUrlParams,
  differences: Omit<InterfaceLaunchUrlResult, 'context' | 'sessionId'>
): InterfaceLaunchUrlResult => transformPayload(params, differences);

/**
 * Interface Launch Url — Type-safe command executor
 *
 * Usage:
 *   import { InterfaceLaunchUrl } from '...shared/InterfaceLaunchUrlTypes';
 *   const result = await InterfaceLaunchUrl.execute({ ... });
 */
export const InterfaceLaunchUrl = {
  execute(params: CommandInput<InterfaceLaunchUrlParams>): Promise<InterfaceLaunchUrlResult> {
    return Commands.execute<InterfaceLaunchUrlParams, InterfaceLaunchUrlResult>('interface/launch/url', params as Partial<InterfaceLaunchUrlParams>);
  },
  commandName: 'interface/launch/url' as const,
} as const;
