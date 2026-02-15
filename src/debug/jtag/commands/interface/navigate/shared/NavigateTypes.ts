// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

import { CommandParams, CommandResult, createPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

export type NavigateTarget = '_blank' | '_self' | '_parent' | '_top' | 'webview' | string;

/** Navigate the browser to a URL. */
export interface NavigateParams extends CommandParams {
  readonly url?: string;  // Optional - if not provided, triggers location.reload()
  readonly timeout?: number;
  readonly waitForSelector?: string;
  readonly target?: NavigateTarget;  // 'webview' to navigate the co-browsing widget
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
/**
 * Navigate — Type-safe command executor
 *
 * Usage:
 *   import { Navigate } from '...shared/NavigateTypes';
 *   const result = await Navigate.execute({ ... });
 */
export const Navigate = {
  execute(params: CommandInput<NavigateParams>): Promise<NavigateResult> {
    return Commands.execute<NavigateParams, NavigateResult>('interface/navigate', params as Partial<NavigateParams>);
  },
  commandName: 'interface/navigate' as const,
} as const;
