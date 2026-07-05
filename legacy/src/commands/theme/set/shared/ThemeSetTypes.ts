/**
 * ThemeSet Types - Theme setting command types
 */

import { type ThemeParams, type ThemeResult, createThemeParams, createThemeResult } from '../../shared/ThemeTypes';
import type { JTAGContext, CommandParams, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { JTAGError } from '../../../../system/core/types/ErrorTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

/** Theme set command parameters */
export interface ThemeSetParams extends CommandParams {
  /** Name of the theme to set */
  readonly themeName: string;
  readonly timestamp?: string;
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
/**
 * ThemeSet — Type-safe command executor
 *
 * Usage:
 *   import { ThemeSet } from '...shared/ThemeSetTypes';
 *   const result = await ThemeSet.execute({ ... });
 */
export const ThemeSet = {
  execute(params: CommandInput<ThemeSetParams>): Promise<ThemeSetResult> {
    return Commands.execute<ThemeSetParams, ThemeSetResult>('theme/set', params as Partial<ThemeSetParams>);
  },
  commandName: 'theme/set' as const,
} as const;
