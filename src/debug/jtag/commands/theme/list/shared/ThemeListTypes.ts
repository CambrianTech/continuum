/**
 * ThemeList Types - Theme listing command types
 */

import { type ThemeParams, type ThemeResult, type ThemeManifest, createThemeParams, createThemeResult } from '../../shared/ThemeTypes';
import type { JTAGContext, CommandParams } from '../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

/** Theme list command parameters */
export interface ThemeListParams extends CommandParams {
  /** Filter by theme category */
  readonly category?: string;
  /** Include full theme manifests in results */
  readonly includeManifests?: boolean;
  readonly timestamp?: string;
}

export const createThemeListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    category?: string;
    includeManifests?: boolean;
    timestamp?: string;
  } = {}
): ThemeListParams => ({
  ...createThemeParams(context, sessionId, data),
  category: data.category,
  includeManifests: data.includeManifests ?? false
});

export interface ThemeListResult extends ThemeResult {
  readonly themes: readonly string[];
  readonly categories: readonly string[];
  readonly currentTheme: string;
  readonly themeCount: number;
  readonly manifests?: readonly ThemeManifest[];
}

export const createThemeListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    themes: readonly string[];
    categories?: readonly string[];
    currentTheme?: string;
    manifests?: readonly ThemeManifest[];
    error?: JTAGError;
    timestamp?: string;
  }
): ThemeListResult => ({
  ...createThemeResult(context, sessionId, data),
  themes: data.themes,
  categories: data.categories ?? [],
  currentTheme: data.currentTheme ?? 'unknown',
  themeCount: data.themes.length,
  ...(data.manifests && { manifests: data.manifests })
});