/**
 * Interface Webmcp Discover Command - Shared Types
 *
 * Discover WebMCP tools available on the current page. Returns structured tool definitions with schemas. Fails explicitly if WebMCP is not available.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * WebMCP Tool definition - matches Chrome 146+ WebMCP spec
 */
export interface WebMCPTool {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

/**
 * Interface Webmcp Discover Command Parameters
 */
export interface InterfaceWebmcpDiscoverParams extends CommandParams {
  // URL to navigate to before discovering tools. If not provided, uses current page.
  url?: string;
}

/**
 * Factory function for creating InterfaceWebmcpDiscoverParams
 */
export const createInterfaceWebmcpDiscoverParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    // URL to navigate to before discovering tools. If not provided, uses current page.
    url?: string;
  }
): InterfaceWebmcpDiscoverParams => createPayload(context, sessionId, {
  url: data.url ?? '',
  ...data
});

/**
 * Interface Webmcp Discover Command Result
 */
export interface InterfaceWebmcpDiscoverResult extends CommandResult {
  success: boolean;
  // Whether WebMCP is available on this page
  available: boolean;
  // Why WebMCP is unavailable (empty if available)
  reason: string;
  // Array of available tools with name, description, and inputSchema
  tools: WebMCPTool[];
  // URL of the page where tools were discovered
  pageUrl: string;
  error?: JTAGError;
}

/**
 * Factory function for creating InterfaceWebmcpDiscoverResult with defaults
 */
export const createInterfaceWebmcpDiscoverResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether WebMCP is available on this page
    available?: boolean;
    // Why WebMCP is unavailable (empty if available)
    reason?: string;
    // Array of available tools with name, description, and inputSchema
    tools?: WebMCPTool[];
    // URL of the page where tools were discovered
    pageUrl?: string;
    error?: JTAGError;
  }
): InterfaceWebmcpDiscoverResult => createPayload(context, sessionId, {
  available: data.available ?? false,
  reason: data.reason ?? '',
  tools: data.tools ?? [],
  pageUrl: data.pageUrl ?? '',
  ...data
});

/**
 * Smart Interface Webmcp Discover-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createInterfaceWebmcpDiscoverResultFromParams = (
  params: InterfaceWebmcpDiscoverParams,
  differences: Omit<InterfaceWebmcpDiscoverResult, 'context' | 'sessionId'>
): InterfaceWebmcpDiscoverResult => transformPayload(params, differences);

/**
 * Interface Webmcp Discover — Type-safe command executor
 *
 * Usage:
 *   import { InterfaceWebmcpDiscover } from '...shared/InterfaceWebmcpDiscoverTypes';
 *   const result = await InterfaceWebmcpDiscover.execute({ ... });
 */
export const InterfaceWebmcpDiscover = {
  execute(params: CommandInput<InterfaceWebmcpDiscoverParams>): Promise<InterfaceWebmcpDiscoverResult> {
    return Commands.execute<InterfaceWebmcpDiscoverParams, InterfaceWebmcpDiscoverResult>('interface/webmcp/discover', params as Partial<InterfaceWebmcpDiscoverParams>);
  },
  commandName: 'interface/webmcp/discover' as const,
} as const;
