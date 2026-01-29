/**
 * ThemeGet Types - Theme getting command types
 */

import { type ThemeParams, type ThemeResult, type ThemeManifest, createThemeParams, createThemeResult } from '../../shared/ThemeTypes';
import type { JTAGContext, CommandParams, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/** Theme get command parameters - gets current theme */
export interface ThemeGetParams extends CommandParams {
  readonly timestamp?: string;
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
/**
 * ThemeGet — Type-safe command executor
 *
 * Usage:
 *   import { ThemeGet } from '...shared/ThemeGetTypes';
 *   const result = await ThemeGet.execute({ ... });
 */
export const ThemeGet = {
  execute(params: CommandInput<ThemeGetParams>): Promise<ThemeGetResult> {
    return Commands.execute<ThemeGetParams, ThemeGetResult>('theme/get', params as Partial<ThemeGetParams>);
  },
  commandName: 'theme/get' as const,
} as const;
