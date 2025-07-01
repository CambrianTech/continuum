/**
 * Core Browser Types and Enums
 */

import { DevToolsCapabilities } from './devtools.js';

// Re-export for external use
export type { DevToolsCapabilities };

export enum BrowserType {
  DEFAULT = 'default',
  CHROME = 'chrome',
  FIREFOX = 'firefox',
  SAFARI = 'safari',
  EDGE = 'edge',
  CHROMIUM = 'chromium',
  OPERA = 'opera'
}

export enum BrowserStatus {
  LAUNCHING = 'launching',
  READY = 'ready',
  ERROR = 'error',
  CLOSING = 'closing',
  CLOSED = 'closed'
}

export enum BrowserPurpose {
  AUTOMATION = 'automation',
  DEVELOPMENT = 'development',
  USER = 'user',
  TESTING = 'testing',
  INTEGRATION_TEST = 'integration_test'
}

export enum BrowserAction {
  LAUNCH = 'launch',
  CLOSE = 'close',
  REFRESH = 'refresh',
  NAVIGATE = 'navigate',
  SCREENSHOT = 'screenshot',
  STATUS = 'status'
}

export interface BrowserConfig {
  type: BrowserType;
  browserType?: BrowserType; // Alias for compatibility
  headless?: boolean;
  devtools?: boolean;
  userDataDir?: string;
  profile?: string;
  args?: string[];
  env?: Record<string, string>;
  timeout?: number;
  purpose?: BrowserPurpose; // Browser usage purpose
  requirements?: { devtools?: boolean; [key: string]: any }; // Browser requirements
  resources?: { [key: string]: any }; // Browser resource configuration
}

export interface SimpleBrowserConfig {
  browserType: BrowserType;
  headless: boolean;
  debugPort: number;
  userDataDir?: string;
  additionalArgs?: string[];
}

export interface BrowserLaunchResult {
  process: any;
  pid: number;
  debugPort: number;
  devToolsUrl?: string;
  capabilities: DevToolsCapabilities;
}

export interface BrowserInfo {
  type: BrowserType;
  version: string;
  executablePath: string;
  userAgent: string;
  isInstalled: boolean;
  capabilities: string[];
}

// DevToolsCapabilities moved to devtools.ts to avoid duplication