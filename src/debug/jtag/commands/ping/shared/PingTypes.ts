/**
 * Ping Command Types - Shared
 * 
 * Basic connectivity testing command with round-trip time measurement
 * and environment detection.
 */

import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';

/**
 * Ping command parameters
 */
export interface PingParams extends JTAGPayload {
  /** Optional message to echo back */
  message?: string;
  /** Include timing information */
  includeTiming?: boolean;
  /** Include environment details */
  includeEnvironment?: boolean;
}

/**
 * Consistent environment information structure
 * Both browser and server use same keys for comparable data
 */
interface BaseEnvironmentInfo {
  type: 'browser' | 'server';
  name: string;              // Browser name or package name
  version: string;           // Browser version or package version
  platform: string;          // OS platform
  runtime: string;           // Runtime info (userAgent or nodeVersion)
  timestamp: string;
}

/**
 * Browser-specific environment information
 */
export interface BrowserEnvironmentInfo extends BaseEnvironmentInfo {
  type: 'browser';
  name: string;              // Extracted from userAgent (Chrome, Safari, Firefox)
  version: string;           // Browser version
  platform: string;          // navigator.platform
  runtime: string;           // Full userAgent string
  language: string;
  online: boolean;
  viewport: {
    width: number;
    height: number;
  };
  screen: {
    width: number;
    height: number;
    colorDepth: number;
  };
  url: string;
  timestamp: string;
}

/**
 * Server-specific environment information
 */
export interface ServerEnvironmentInfo extends BaseEnvironmentInfo {
  type: 'server';
  name: string;              // Package name
  version: string;           // Package version
  platform: string;          // OS platform (darwin, linux, win32)
  runtime: string;           // Node.js version
  arch: string;
  processId: number;
  uptime: number;
  memory: {
    used: number;
    total: number;
    usage: string;
  };
  health: {
    browsersConnected: number;
    commandsRegistered: number;
    daemonsActive: number;
    systemReady: boolean;
  };
  timestamp: string;
}

/**
 * Union type for environment information
 */
export type EnvironmentInfo = BrowserEnvironmentInfo | ServerEnvironmentInfo;

/**
 * Complete system health from both environments
 */
export interface SystemHealth {
  server: ServerEnvironmentInfo;
  browser?: BrowserEnvironmentInfo;  // Optional if browser not connected
}

/**
 * Ping command result
 */
export interface PingResult extends JTAGPayload {
  /** Success status */
  success: boolean;
  /** Echo of the original message */
  message?: string;
  /** Round-trip time in milliseconds */
  roundTripTime?: number;
  /** Rich environment information */
  environment?: EnvironmentInfo;
  /** Any error message */
  error?: string;
}

/**
 * Create ping parameters with defaults
 */
export function createPingParams(
  context: JTAGContext,
  sessionId: UUID,
  options: Partial<PingParams> = {}
): PingParams {
  return {
    context,
    sessionId,
    message: options.message ?? 'ping',
    includeTiming: options.includeTiming ?? true,
    includeEnvironment: options.includeEnvironment ?? true,
    ...options
  };
}

/**
 * Create ping result from params and execution data
 */
export function createPingResultFromParams(
  params: PingParams,
  data: {
    success: boolean;
    roundTripTime?: number;
    environment?: PingResult['environment'];
    error?: string;
  }
): PingResult {
  return {
    context: params.context,
    sessionId: params.sessionId,
    success: data.success,
    message: params.message,
    roundTripTime: data.roundTripTime,
    environment: data.environment,
    error: data.error
  };
}