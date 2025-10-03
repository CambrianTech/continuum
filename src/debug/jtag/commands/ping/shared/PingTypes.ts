import type { JTAGPayload } from '../../../system/core/types/JTAGTypes';

export interface PingParams extends JTAGPayload {
  server?: ServerEnvironmentInfo;
  browser?: BrowserEnvironmentInfo;
}

export interface ServerEnvironmentInfo {
  type: 'server';
  name: string;
  version: string;
  runtime: string;
  platform: string;
  arch: string;
  processId: number;
  uptime: number;
  memory: { used: number; total: number; usage: string };
  health: {
    browsersConnected: number;
    commandsRegistered: number;
    daemonsActive: number;
    systemReady: boolean;
  };
  timestamp: string;
}

export interface BrowserEnvironmentInfo {
  type: 'browser';
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

export interface PingResult extends JTAGPayload {
  success: boolean;
  server?: ServerEnvironmentInfo;
  browser?: BrowserEnvironmentInfo;
  timestamp: string;
}
