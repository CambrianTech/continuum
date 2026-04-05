/**
 * Grid Setup Check Command - Shared Types
 *
 * Diagnose grid setup: Tailscale install, connectivity, HTTPS certs, peers,
 * Docker grid profile, and actionable fix steps.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/** Individual diagnostic check result */
export interface GridSetupCheck_DiagnosticCheck {
  check: string;
  status: 'pass' | 'fail' | 'warn' | 'info' | 'skip';
  detail: string;
  peers?: string[];
}

/**
 * Grid Setup Check Command Parameters
 */
export interface GridSetupCheckParams extends CommandParams {
  _noParams?: never;
}

/**
 * Factory function for creating GridSetupCheckParams
 */
export const createGridSetupCheckParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Record<string, unknown> = {}
): GridSetupCheckParams => createPayload(context, sessionId, {
  ...data
}) as unknown as GridSetupCheckParams;

/**
 * Grid Setup Check Command Result
 */
export interface GridSetupCheckResult extends CommandResult {
  success: boolean;
  ready: boolean;
  tailscaleIp: string | null;
  dnsName: string | null;
  peerCount: number;
  checks: GridSetupCheck_DiagnosticCheck[];
  actions: string[];
  summary: string;
  error?: JTAGError;
}

/**
 * Factory function for creating GridSetupCheckResult with defaults
 */
export const createGridSetupCheckResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    ready?: boolean;
    tailscaleIp?: string | null;
    dnsName?: string | null;
    peerCount?: number;
    checks?: GridSetupCheck_DiagnosticCheck[];
    actions?: string[];
    summary?: string;
    error?: JTAGError;
  }
): GridSetupCheckResult => createPayload(context, sessionId, {
  ready: data.ready ?? false,
  tailscaleIp: data.tailscaleIp ?? null,
  dnsName: data.dnsName ?? null,
  peerCount: data.peerCount ?? 0,
  checks: data.checks ?? [],
  actions: data.actions ?? [],
  summary: data.summary ?? '',
  ...data
});

/**
 * Smart Grid Setup Check-specific inheritance from params
 * Auto-inherits context and sessionId from params
 */
export const createGridSetupCheckResultFromParams = (
  params: GridSetupCheckParams,
  differences: Omit<GridSetupCheckResult, 'context' | 'sessionId' | 'userId'>
): GridSetupCheckResult => transformPayload(params, differences);

/**
 * Grid Setup Check — Type-safe command executor
 *
 * Usage:
 *   import { GridSetupCheck } from '...shared/GridSetupCheckTypes';
 *   const result = await GridSetupCheck.execute({ ... });
 */
export const GridSetupCheck = {
  execute(params: CommandInput<GridSetupCheckParams>): Promise<GridSetupCheckResult> {
    return Commands.execute<GridSetupCheckParams, GridSetupCheckResult>('grid/setup-check', params as Partial<GridSetupCheckParams>);
  },
  commandName: 'grid/setup-check' as const,
} as const;
