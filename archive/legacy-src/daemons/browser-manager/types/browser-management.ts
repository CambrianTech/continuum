/**
 * Browser Management and Connection Types
 */

import { BrowserType, BrowserStatus, BrowserConfig, DevToolsCapabilities, BrowserAction } from './browser-core.js';

export interface BrowserRequest {
  action: BrowserAction;
  browserType?: BrowserType;
  url?: string;
  options?: BrowserConfig;
  sessionId?: string;
  filters?: BrowserFilters;
}

export interface BrowserConnection {
  id: string;
  type: BrowserType;
  status: BrowserStatus;
  pid?: number;
  debugPort?: number;
  devToolsUrl?: string;
  capabilities: DevToolsCapabilities;
  lastActivity: Date;
  metadata: Record<string, any>;
}

export interface ManagedBrowser {
  id: string;
  type: BrowserType;
  pid: number;
  debugPort: number;
  port?: number; // Additional port property
  status: BrowserStatus;
  state?: any; // Browser state
  devToolsUrl?: string;
  launchedAt: Date;
  lastActivity: Date;
  config: BrowserConfig;
  process?: any;
  resources?: {
    memory: number;
    cpu: number;
    handles: number;
  };
  sessions?: string[];
}

export interface BrowserFilters {
  type?: BrowserType;
  status?: BrowserStatus;
  minIdleTime?: number;
  maxAge?: number;
  hasDevTools?: boolean;
}