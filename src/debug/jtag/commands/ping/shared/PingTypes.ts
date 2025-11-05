import type { CommandParams, CommandResult } from '../../../system/core/types/JTAGTypes';

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
