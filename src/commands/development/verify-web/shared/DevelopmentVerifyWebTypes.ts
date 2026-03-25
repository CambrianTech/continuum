/**
 * Development Verify Web Command - Shared Types
 *
 * Verify web output by opening in headless Playwright browser, capturing console errors + screenshot. Used by Academy teacher to grade coding output. No blind training.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Development Verify Web Command Parameters
 */
export interface DevelopmentVerifyWebParams extends CommandParams {
  // Path to HTML file to verify
  filePath: string;
  // URL to verify (alternative to filePath)
  url: string;
  // Time to wait after page load before capturing (default: 2000)
  waitMs: number;
  // Take screenshot (default: true)
  screenshot: boolean;
  // Screenshot output path (default: auto-generated)
  screenshotPath: string;
  // Capture all console output (default: true)
  captureConsole: boolean;
  // Viewport size WxH (default: 1280x720)
  viewport: string;
}

/**
 * Factory function for creating DevelopmentVerifyWebParams
 */
export const createDevelopmentVerifyWebParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Path to HTML file to verify
    filePath: string;
    // URL to verify (alternative to filePath)
    url: string;
    // Time to wait after page load before capturing (default: 2000)
    waitMs: number;
    // Take screenshot (default: true)
    screenshot: boolean;
    // Screenshot output path (default: auto-generated)
    screenshotPath: string;
    // Capture all console output (default: true)
    captureConsole: boolean;
    // Viewport size WxH (default: 1280x720)
    viewport: string;
  }
): DevelopmentVerifyWebParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,

  ...data
});

/**
 * Development Verify Web Command Result
 */
export interface DevelopmentVerifyWebResult extends CommandResult {
  // True if page loaded with zero errors
  success: boolean;
  // Runtime JavaScript errors captured from page
  errors: string[];
  // All console.log/warn/error messages
  consoleOutput: string[];
  // Path to captured screenshot
  screenshotPath: string;
  // Base64-encoded screenshot for AI vision
  screenshotBase64: string;
  // Document title after load
  pageTitle: string;
  // Page load time in milliseconds
  loadTimeMs: number;
  error?: JTAGError;
}

/**
 * Factory function for creating DevelopmentVerifyWebResult with defaults
 */
export const createDevelopmentVerifyWebResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // True if page loaded with zero errors
    success: boolean;
    // Runtime JavaScript errors captured from page
    errors?: string[];
    // All console.log/warn/error messages
    consoleOutput?: string[];
    // Path to captured screenshot
    screenshotPath?: string;
    // Base64-encoded screenshot for AI vision
    screenshotBase64?: string;
    // Document title after load
    pageTitle?: string;
    // Page load time in milliseconds
    loadTimeMs?: number;
    error?: JTAGError;
  }
): DevelopmentVerifyWebResult => createPayload(context, sessionId, {
  success: data.success,
  errors: data.errors ?? [],
  consoleOutput: data.consoleOutput ?? [],
  screenshotPath: data.screenshotPath ?? '',
  screenshotBase64: data.screenshotBase64 ?? '',
  pageTitle: data.pageTitle ?? '',
  loadTimeMs: data.loadTimeMs ?? 0,
});

/**
 * Smart Development Verify Web-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createDevelopmentVerifyWebResultFromParams = (
  params: DevelopmentVerifyWebParams,
  differences: Omit<DevelopmentVerifyWebResult, 'context' | 'sessionId' | 'userId'>
): DevelopmentVerifyWebResult => transformPayload(params, differences);

/**
 * Development Verify Web — Type-safe command executor
 *
 * Usage:
 *   import { DevelopmentVerifyWeb } from '...shared/DevelopmentVerifyWebTypes';
 *   const result = await DevelopmentVerifyWeb.execute({ ... });
 */
export const DevelopmentVerifyWeb = {
  execute(params: CommandInput<DevelopmentVerifyWebParams>): Promise<DevelopmentVerifyWebResult> {
    return Commands.execute<DevelopmentVerifyWebParams, DevelopmentVerifyWebResult>('development/verify-web', params as Partial<DevelopmentVerifyWebParams>);
  },
  commandName: 'development/verify-web' as const,
} as const;
