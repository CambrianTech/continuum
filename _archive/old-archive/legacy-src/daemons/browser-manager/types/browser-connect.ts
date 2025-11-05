/**
 * Browser Connect Configuration - Universal client interface
 * 
 * This is the config struct that ALL clients use when calling connect():
 * - ./continuum connect [options]
 * - ai-portal.py calls connect(config)  
 * - Any other integration
 * 
 * The BrowserManagerDaemon uses this config to make intelligent decisions
 * about whether to reuse existing browsers or launch new ones.
 */

import { BrowserType } from './browser-core.js';

export interface BrowserConnectConfig {
  /**
   * Browser type to use
   * - 'default': Use system default browser
   * - 'chrome', 'firefox', 'opera', etc.: Use specific browser
   * 
   * If different from current session browser, will launch new browser
   */
  browser?: BrowserType;

  /**
   * Session management
   * - 'shared': Use shared session (default) - reuses existing browser
   * - 'new': Force new session - always launches new browser  
   * - string: Specific session ID - reuses if exists, creates if not
   */
  session?: 'shared' | 'new' | string;

  /**
   * DevTools configuration
   * - true: Open DevTools panel (may reuse browser but open devtools)
   * - false: Normal browser window (default)
   */
  devtools?: boolean;

  /**
   * Headless mode
   * - true: Run browser in headless mode (no GUI)
   * - false: Normal browser window (default)
   */
  headless?: boolean;

  /**
   * Browser profile/user data directory
   * - string: Use specific profile (different profile = new browser)
   * - undefined: Use default profile
   */
  profile?: string;

  /**
   * URL to open
   * - string: Specific URL to navigate to
   * - undefined: Use default (http://localhost:9000)
   */
  url?: string;

  /**
   * Window positioning and size (for multi-screen setups)
   */
  window?: {
    width?: number;
    height?: number;
    x?: number;
    y?: number;
  };

  /**
   * Additional browser arguments
   * - For power users who need specific browser flags
   */
  args?: string[];

  /**
   * Force new browser launch even if existing one matches criteria
   * - true: Always launch new browser regardless of existing ones
   * - false: Use intelligent reuse logic (default)
   */
  forceNew?: boolean;
}

/**
 * Default configuration used when no config is provided
 */
export const DEFAULT_BROWSER_CONNECT_CONFIG: BrowserConnectConfig = {
  browser: BrowserType.DEFAULT,
  session: 'shared',
  devtools: false,
  headless: false,
  url: 'http://localhost:9000',
  forceNew: false
};

/**
 * Merge user config with defaults
 */
export function mergeBrowserConnectConfig(
  userConfig: Partial<BrowserConnectConfig> = {}
): BrowserConnectConfig {
  return {
    ...DEFAULT_BROWSER_CONNECT_CONFIG,
    ...userConfig
  };
}

/**
 * Determine if two configs would result in the same browser instance
 * Used by BrowserManagerDaemon to decide whether to reuse or launch new
 */
export function configsRequireSameBrowser(
  config1: BrowserConnectConfig,
  config2: BrowserConnectConfig
): boolean {
  return (
    config1.browser === config2.browser &&
    config1.profile === config2.profile &&
    config1.headless === config2.headless &&
    !config1.forceNew &&
    !config2.forceNew
  );
}

/**
 * Browser Connect Decision - what the daemon should do
 */
export interface BrowserConnectDecision {
  action: 'reuse' | 'launch_new' | 'open_devtools' | 'navigate';
  browserId?: string; // ID of browser to reuse (if action is 'reuse')
  reason: string; // Human-readable explanation of decision
  config: BrowserConnectConfig; // Final resolved config
}