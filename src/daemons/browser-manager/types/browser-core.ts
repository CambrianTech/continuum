/**
 * Core Browser Types and Enums
 */

export enum BrowserType {
  DEFAULT = 'default',
  CHROME = 'chrome',
  FIREFOX = 'firefox',
  SAFARI = 'safari',
  EDGE = 'edge',
  CHROMIUM = 'chromium',
  OPERA = 'opera'
}

export type BrowserStatus = 'launching' | 'ready' | 'error' | 'closing' | 'closed';

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
  purpose?: string; // Browser usage purpose
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

export interface DevToolsCapabilities {
  supportsConsole: boolean;
  supportsNetwork: boolean;
  supportsPerformance: boolean;
  supportsScreenshot: boolean;
  supportsProfiling: boolean;
  supportsCodeCoverage: boolean;
  supportsSecurityAnalysis: boolean;
}