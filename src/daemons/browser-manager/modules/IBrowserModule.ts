/**
 * Browser Module Interface - Drop-in browser integration modules
 * 
 * Each browser type implements this interface to provide:
 * - DevTools integration (browser-specific protocols)
 * - Launch configuration and arguments
 * - Tab management and automation
 * - Platform-specific capabilities
 */

import { BrowserConfig, ManagedBrowser, BrowserType } from '../BrowserManagerDaemon';

export interface DevToolsCapabilities {
  protocol: 'chrome-devtools' | 'safari-inspector' | 'firefox-marionette' | 'edge-devtools';
  supportsHeadless: boolean;
  supportsRemoteDebugging: boolean;
  supportsAutomation: boolean;
  supportsExtensions: boolean;
  defaultPort: number;
  portRange: [number, number];
}

export interface BrowserLaunchResult {
  process: any; // Child process
  pid: number;
  debugPort: number;
  devToolsUrl?: string;
  capabilities: DevToolsCapabilities;
}

export interface TabManagementAPI {
  createTab(url: string): Promise<string>; // Returns tab ID
  closeTab(tabId: string): Promise<void>;
  refreshTab(tabId: string): Promise<void>;
  navigateTab(tabId: string, url: string): Promise<void>;
  listTabs(): Promise<Array<{ id: string; url: string; title: string }>>;
}

export interface IBrowserModule {
  readonly browserType: BrowserType;
  readonly capabilities: DevToolsCapabilities;
  
  /**
   * Detect if this browser is available on the system
   */
  isAvailable(): Promise<boolean>;
  
  /**
   * Get browser binary path
   */
  getBinaryPath(): Promise<string | null>;
  
  /**
   * Build launch arguments for this browser type
   */
  buildLaunchArgs(config: BrowserConfig, debugPort: number): string[];
  
  /**
   * Launch browser process with given configuration
   */
  launch(config: BrowserConfig, debugPort: number): Promise<BrowserLaunchResult>;
  
  /**
   * Wait for browser to be ready (DevTools responsive)
   */
  waitForReady(browser: ManagedBrowser): Promise<void>;
  
  /**
   * Get tab management API for this browser
   */
  getTabAPI(browser: ManagedBrowser): Promise<TabManagementAPI>;
  
  /**
   * Terminate browser process
   */
  terminate(browser: ManagedBrowser): Promise<void>;
  
  /**
   * Check if browser process is healthy
   */
  isHealthy(browser: ManagedBrowser): Promise<boolean>;
  
  /**
   * Get browser-specific monitoring data
   */
  getMetrics(browser: ManagedBrowser): Promise<{
    tabs: number;
    memory: number;
    cpu: number;
    devToolsConnections: number;
  }>;
}