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
 * Browser-specific environment information
 */
export interface BrowserEnvironmentInfo {
  type: 'browser';
  userAgent: string;
  platform: string;
  language: string;
  cookieEnabled: boolean;
  onLine: boolean;
  screenResolution: {
    width: number;
    height: number;
    colorDepth: number;
  };
  viewport: {
    width: number;
    height: number;
  };
  url: string;
  timestamp: string;
}

/**
 * Server-specific environment information
 */
export interface ServerEnvironmentInfo {
  type: 'server';
  nodeVersion: string;
  platform: string;
  arch: string;
  processId: number;
  uptime: number;
  memory: {
    used: number;
    total: number;
    usage: string;
  };
  cpuUsage: {
    user: number;
    system: number;
  };
  // System health information
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
    message: options.message || 'ping',
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