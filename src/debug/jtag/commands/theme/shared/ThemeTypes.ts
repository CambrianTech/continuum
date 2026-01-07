/**
 * Theme Command Types - Base types for theme operations
 * 
 * Following the same pattern as FileTypes for consistency
 */

import type { JTAGContext, CommandParams } from '../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * Base theme parameters interface
 */
export interface ThemeParams extends CommandParams {
  readonly timestamp?: string;
}

/**
 * Base theme result interface
 */
export interface ThemeResult {
  readonly success: boolean;
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  readonly timestamp: string;
  readonly error?: JTAGError;
}

/**
 * Theme manifest interface for discovered themes
 */
export interface ThemeManifest {
  readonly name: string;
  readonly displayName: string;
  readonly description: string;
  readonly category: string;
  readonly author: string;
  readonly version: string;
  readonly files: readonly string[];
  readonly tags: readonly string[];
  readonly preview: {
    readonly primaryColor: string;
    readonly backgroundColor: string;
    readonly textColor: string;
  };
}

/**
 * Create base theme parameters
 */
export function createThemeParams(
  context: JTAGContext,
  sessionId: UUID,
  data: {
    timestamp?: string;
  } = {}
): ThemeParams {
  return {
    context,
    sessionId,
    timestamp: data.timestamp || new Date().toISOString()
  };
}

/**
 * Create base theme result
 */
export function createThemeResult(
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    error?: JTAGError;
    timestamp?: string;
  }
): ThemeResult {
  return {
    context,
    sessionId,
    timestamp: data.timestamp || new Date().toISOString(),
    success: data.success,
    ...(data.error && { error: data.error })
  };
}