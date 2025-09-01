/**
 * ThemeGet Types - Theme getting command types
 */

import { type ThemeParams, type ThemeResult, type ThemeManifest, createThemeParams, createThemeResult } from '../../shared/ThemeTypes';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface ThemeGetParams extends ThemeParams {
  // No additional parameters needed - gets current theme
}

export const createThemeGetParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    timestamp?: string;
  } = {}
): ThemeGetParams => createThemeParams(context, sessionId, data);

export interface ThemeGetResult extends ThemeResult {
  readonly currentTheme: string;
  readonly themeManifest?: ThemeManifest;
  readonly themeApplied: boolean;
}

export const createThemeGetResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    currentTheme: string;
    themeManifest?: ThemeManifest;
    themeApplied?: boolean;
    error?: JTAGError;
    timestamp?: string;
  }
): ThemeGetResult => ({
  ...createThemeResult(context, sessionId, data),
  currentTheme: data.currentTheme,
  themeManifest: data.themeManifest,
  themeApplied: data.themeApplied ?? true
});