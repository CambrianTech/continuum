/**
 * Interface Webmcp Call Command - Shared Types
 *
 * Call a WebMCP tool on the current page. Returns structured result from the tool. Fails explicitly if WebMCP is not available or tool not found.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Interface Webmcp Call Command Parameters
 */
export interface InterfaceWebmcpCallParams extends CommandParams {
  // Name of the tool to call (from discover results)
  toolName: string;
  // Parameters to pass to the tool (must match tool's inputSchema)
  params: object;
  // URL to navigate to before calling tool. If not provided, uses current page.
  url?: string;
}

/**
 * Factory function for creating InterfaceWebmcpCallParams
 */
export const createInterfaceWebmcpCallParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // Name of the tool to call (from discover results)
    toolName: string;
    // Parameters to pass to the tool (must match tool's inputSchema)
    params: object;
    // URL to navigate to before calling tool. If not provided, uses current page.
    url?: string;
  }
): InterfaceWebmcpCallParams => createPayload(context, sessionId, {
  url: data.url ?? '',
  ...data
});

/**
 * Interface Webmcp Call Command Result
 */
export interface InterfaceWebmcpCallResult extends CommandResult {
  success: boolean;
  // Whether the tool was successfully called
  called: boolean;
  // Why the call failed (empty if successful)
  reason: string;
  // Name of the tool that was called
  toolName: string;
  // Result returned by the tool
  result: unknown;
  // URL of the page where tool was called
  pageUrl: string;
  error?: JTAGError;
}

/**
 * Factory function for creating InterfaceWebmcpCallResult with defaults
 */
export const createInterfaceWebmcpCallResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether the tool was successfully called
    called?: boolean;
    // Why the call failed (empty if successful)
    reason?: string;
    // Name of the tool that was called
    toolName?: string;
    // Result returned by the tool
    result?: unknown;
    // URL of the page where tool was called
    pageUrl?: string;
    error?: JTAGError;
  }
): InterfaceWebmcpCallResult => createPayload(context, sessionId, {
  called: data.called ?? false,
  reason: data.reason ?? '',
  toolName: data.toolName ?? '',
  result: data.result ?? undefined,
  pageUrl: data.pageUrl ?? '',
  ...data
});

/**
 * Smart Interface Webmcp Call-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createInterfaceWebmcpCallResultFromParams = (
  params: InterfaceWebmcpCallParams,
  differences: Omit<InterfaceWebmcpCallResult, 'context' | 'sessionId'>
): InterfaceWebmcpCallResult => transformPayload(params, differences);

/**
 * Interface Webmcp Call — Type-safe command executor
 *
 * Usage:
 *   import { InterfaceWebmcpCall } from '...shared/InterfaceWebmcpCallTypes';
 *   const result = await InterfaceWebmcpCall.execute({ ... });
 */
export const InterfaceWebmcpCall = {
  execute(params: CommandInput<InterfaceWebmcpCallParams>): Promise<InterfaceWebmcpCallResult> {
    return Commands.execute<InterfaceWebmcpCallParams, InterfaceWebmcpCallResult>('interface/webmcp/call', params as Partial<InterfaceWebmcpCallParams>);
  },
  commandName: 'interface/webmcp/call' as const,
} as const;
