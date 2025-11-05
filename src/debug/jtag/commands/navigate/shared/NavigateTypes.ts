// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Navigate Command - Shared Types for Browser Navigation
 * 
 * Minimal, focused types for URL navigation across browser/server contexts.
 * Follows the elegant pattern of screenshot command - simple params and results
 * with clean inheritance from CommandParams/CommandResult base classes.
 * 
 * DESIGN PRINCIPLES:
 * - Object.assign() in constructor for clean initialization
 * - Optional properties with sensible defaults
 * - Environment-aware results with timestamps
 * - Type safety without overkill complexity
 * 
 * USAGE:
 * - Browser: Direct window.location navigation
 * - Server: Delegates to browser context
 * - Symmetric interface across both contexts
 */

import { CommandParams, CommandResult, createPayload } from '../../../system/core/types/JTAGTypes';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

export interface NavigateParams extends CommandParams {
  readonly url: string;
  readonly timeout?: number;
  readonly waitForSelector?: string;
  readonly target?: '_blank' | '_self' | '_parent' | '_top' | string;
}

export const createNavigateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    url?: string;
    timeout?: number;
    waitForSelector?: string;
  }
): NavigateParams => createPayload(context, sessionId, {
  url: data.url ?? '',
  timeout: data.timeout ?? 30000,
  waitForSelector: data.waitForSelector,
  ...data
});

export interface NavigateResult extends CommandResult {
  readonly success: boolean;
  readonly url: string;
  readonly title?: string;
  readonly loadTime?: number;
  readonly error?: JTAGError;
  readonly timestamp: string;
}

export const createNavigateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    url?: string;
    title?: string;
    loadTime?: number;
    error?: JTAGError;
  }
): NavigateResult => createPayload(context, sessionId, {
  url: data.url ?? '',
  title: data.title,
  loadTime: data.loadTime,
  timestamp: new Date().toISOString(),
  ...data
});