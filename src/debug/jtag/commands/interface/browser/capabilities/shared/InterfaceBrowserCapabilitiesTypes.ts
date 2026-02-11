/**
 * Interface Browser Capabilities Command - Shared Types
 *
 * Check available browser automation capabilities. Returns explicit status for each capability (webmcp, puppeteer, etc). No fallbacks - AIs see exactly what is/isn't available.
 */

import type { CommandParams, CommandResult, CommandInput, JTAGContext } from '@system/core/types/JTAGTypes';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { JTAGError } from '@system/core/types/ErrorTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Interface Browser Capabilities Command Parameters
 */
export interface InterfaceBrowserCapabilitiesParams extends CommandParams {
  _noParams?: never; // Marker to avoid empty interface
}

/**
 * Factory function for creating InterfaceBrowserCapabilitiesParams
 */
export const createInterfaceBrowserCapabilitiesParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Record<string, never>
): InterfaceBrowserCapabilitiesParams => createPayload(context, sessionId, {

  ...data
});

/**
 * Interface Browser Capabilities Command Result
 */
export interface InterfaceBrowserCapabilitiesResult extends CommandResult {
  success: boolean;
  // Whether WebMCP (navigator.modelContext) is available
  webmcp: boolean;
  // Why WebMCP is unavailable (empty if available)
  webmcpReason: string;
  // Whether Puppeteer automation is available
  puppeteer: boolean;
  // Why Puppeteer is unavailable (empty if available)
  puppeteerReason: string;
  // Whether form introspection (via puppeteer) is available
  formIntrospection: boolean;
  // Whether system browser (open/xdg-open) is available
  systemBrowser: boolean;
  // List of available browser automation backends
  availableBackends: string[];
  // Guidance on what capabilities to use
  hint: string;
  error?: JTAGError;
}

/**
 * Factory function for creating InterfaceBrowserCapabilitiesResult with defaults
 */
export const createInterfaceBrowserCapabilitiesResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: {
    success: boolean;
    // Whether WebMCP (navigator.modelContext) is available
    webmcp?: boolean;
    // Why WebMCP is unavailable (empty if available)
    webmcpReason?: string;
    // Whether Puppeteer automation is available
    puppeteer?: boolean;
    // Why Puppeteer is unavailable (empty if available)
    puppeteerReason?: string;
    // Whether form introspection is available
    formIntrospection?: boolean;
    // Whether system browser (open/xdg-open) is available
    systemBrowser?: boolean;
    // List of available browser automation backends
    availableBackends?: string[];
    // Guidance on capabilities
    hint?: string;
    error?: JTAGError;
  }
): InterfaceBrowserCapabilitiesResult => createPayload(context, sessionId, {
  webmcp: data.webmcp ?? false,
  webmcpReason: data.webmcpReason ?? '',
  puppeteer: data.puppeteer ?? false,
  puppeteerReason: data.puppeteerReason ?? '',
  formIntrospection: data.formIntrospection ?? false,
  systemBrowser: data.systemBrowser ?? false,
  availableBackends: data.availableBackends ?? [],
  hint: data.hint ?? '',
  ...data
});

/**
 * Smart Interface Browser Capabilities-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createInterfaceBrowserCapabilitiesResultFromParams = (
  params: InterfaceBrowserCapabilitiesParams,
  differences: Omit<InterfaceBrowserCapabilitiesResult, 'context' | 'sessionId'>
): InterfaceBrowserCapabilitiesResult => transformPayload(params, differences);

/**
 * Interface Browser Capabilities — Type-safe command executor
 *
 * Usage:
 *   import { InterfaceBrowserCapabilities } from '...shared/InterfaceBrowserCapabilitiesTypes';
 *   const result = await InterfaceBrowserCapabilities.execute({ ... });
 */
export const InterfaceBrowserCapabilities = {
  execute(params: CommandInput<InterfaceBrowserCapabilitiesParams>): Promise<InterfaceBrowserCapabilitiesResult> {
    return Commands.execute<InterfaceBrowserCapabilitiesParams, InterfaceBrowserCapabilitiesResult>('interface/browser/capabilities', params as Partial<InterfaceBrowserCapabilitiesParams>);
  },
  commandName: 'interface/browser/capabilities' as const,
} as const;
