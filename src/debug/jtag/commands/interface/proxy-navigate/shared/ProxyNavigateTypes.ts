/**
 * Proxy Navigate Command - Shared Types
 * 
 * Enables cross-origin navigation through proxy system for widget training.
 * Solves the fundamental html2canvas + cross-origin iframe limitation.
 */

import { CommandParams, CommandResult, createPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

export interface ProxyNavigateParams extends CommandParams {
  readonly url: string;
  readonly target?: string;
  readonly rewriteUrls?: boolean;
  readonly userAgent?: string;
  readonly timeout?: number;
}

export interface ProxyNavigateResult extends CommandResult {
  readonly success: boolean;
  readonly proxyUrl: string;
  readonly originalUrl: string;
  readonly statusCode?: number;
  readonly loadTime?: number;
  readonly error?: JTAGError;
}

export const createProxyNavigateParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    url: string;
    target?: string;
    rewriteUrls?: boolean;
    userAgent?: string;
    timeout?: number;
  }
): ProxyNavigateParams => createPayload(context, sessionId, {
  target: data.target || 'proxy-iframe',
  rewriteUrls: data.rewriteUrls ?? true,
  userAgent: data.userAgent || 'Continuum-Training-Bot/1.0',
  timeout: data.timeout ?? 30000,
  ...data
});

export const createProxyNavigateResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    proxyUrl?: string;
    originalUrl?: string;
    statusCode?: number;
    loadTime?: number;
    error?: JTAGError;
  }
): ProxyNavigateResult => createPayload(context, sessionId, {
  proxyUrl: data.proxyUrl || '',
  originalUrl: data.originalUrl || '',
  ...data
});
/**
 * ProxyNavigate — Type-safe command executor
 *
 * Usage:
 *   import { ProxyNavigate } from '...shared/ProxyNavigateTypes';
 *   const result = await ProxyNavigate.execute({ ... });
 */
export const ProxyNavigate = {
  execute(params: CommandInput<ProxyNavigateParams>): Promise<ProxyNavigateResult> {
    return Commands.execute<ProxyNavigateParams, ProxyNavigateResult>('interface/proxy-navigate', params as Partial<ProxyNavigateParams>);
  },
  commandName: 'interface/proxy-navigate' as const,
} as const;
