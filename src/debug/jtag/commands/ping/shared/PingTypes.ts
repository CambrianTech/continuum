import type { CommandParams, CommandResult, CommandInput } from '../../../system/core/types/JTAGTypes';
import { Commands } from '../../../system/core/shared/Commands';

export interface PingParams extends CommandParams {
  server?: ServerEnvironmentInfo;
  browser?: BrowserEnvironmentInfo;

  /**
   * Include detailed AI persona health status
   * If true, calls ai/status with timeout and includes summary
   */
  verbose?: boolean;
}

export interface ServerEnvironmentInfo {
  type: 'server';
  packageName: string;
  packageVersion: string;
  name: string;
  version: string;
  runtime: string;
  platform: string;
  arch: string;
  processId: number;
  uptime: number;
  memory: { used: number; total: number; usage: string };
  health: {
    commandsRegistered: number;
    daemonsActive: number;
    systemReady: boolean;
  };
  timestamp: string;
}

export interface BrowserEnvironmentInfo {
  type: 'browser';
  packageName: string;
  packageVersion: string;
  name: string;
  version: string;
  runtime: string;
  platform: string;
  language: string;
  online: boolean;
  viewport: { width: number; height: number };
  screen: { width: number; height: number; colorDepth: number };
  url: string;
  timestamp: string;
}

export interface PingResult extends CommandResult {
  success: boolean;
  server?: ServerEnvironmentInfo;
  browser?: BrowserEnvironmentInfo;
  timestamp: string;

  /**
   * AI persona health summary (if verbose=true)
   */
  aiStatus?: {
    total: number;
    healthy: number;
    starting: number;
    degraded: number;
    dead: number;
    checkDuration?: number;  // Milliseconds taken to check status
  };
}

/**
 * Ping — Type-safe command executor
 *
 * Usage:
 *   import { Ping } from '@commands/ping/shared/PingTypes';
 *   const result = await Ping.execute({ verbose: true });
 */
export const Ping = {
  execute(params?: CommandInput<PingParams>): Promise<PingResult> {
    return Commands.execute<PingParams, PingResult>('ping', params as Partial<PingParams>);
  },
  commandName: 'ping' as const,
} as const;
