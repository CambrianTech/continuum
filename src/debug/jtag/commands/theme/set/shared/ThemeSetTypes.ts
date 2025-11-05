/**
 * ThemeSet Types - Theme setting command types
 */

import { type ThemeParams, type ThemeResult, createThemeParams, createThemeResult } from '../../shared/ThemeTypes';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface ThemeSetParams extends ThemeParams {
  readonly themeName: string;
}

export const createThemeSetParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    themeName: string;
    timestamp?: string;
  }
): ThemeSetParams => ({
  ...createThemeParams(context, sessionId, data),
  themeName: data.themeName
});

export interface ThemeSetResult extends ThemeResult {
  readonly themeName: string;
  readonly previousTheme?: string;
  readonly applied: boolean;
}

export const createThemeSetResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    themeName: string;
    previousTheme?: string;
    applied?: boolean;
    error?: JTAGError;
    timestamp?: string;
  }
): ThemeSetResult => ({
  ...createThemeResult(context, sessionId, data),
  themeName: data.themeName,
  previousTheme: data.previousTheme,
  applied: data.applied ?? data.success
});