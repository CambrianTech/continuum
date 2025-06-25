/**
 * Base browser adapter - common functionality for all browsers
 * Subclasses implement browser-specific behavior
 */

import { IBrowserAdapter, BrowserLaunchConfig } from './IBrowserAdapter.js';
import { promises as fs } from 'fs';

export abstract class BaseBrowserAdapter implements IBrowserAdapter {
  abstract readonly name: string;
  abstract readonly executablePaths: string[];
  abstract readonly supportsHeadless: boolean;
  abstract readonly supportsRemoteDebugging: boolean;

  /**
   * Get first available executable path
   */
  getExecutablePath(): string | null {
    for (const path of this.executablePaths) {
      try {
        // Check if file exists and is executable
        if (this.fileExists(path)) {
          return path;
        }
      } catch (error) {
        // Continue to next path
      }
    }
    return null;
  }

  /**
   * Check if browser is available on system
   */
  async isAvailable(): Promise<boolean> {
    const execPath = this.getExecutablePath();
    return execPath !== null;
  }

  /**
   * Standard DevTools endpoint for Chromium-based browsers
   */
  getDevToolsEndpoint(port: number): string {
    return `http://localhost:${port}/json`;
  }

  /**
   * Standard ready detection for Chromium-based browsers
   */
  async isReady(port: number): Promise<boolean> {
    try {
      const response = await fetch(this.getDevToolsEndpoint(port), {
        signal: AbortSignal.timeout(2000)
      });
      return response.ok;
    } catch (error) {
      return false;
    }
  }

  /**
   * Build common arguments shared by most browsers
   */
  protected buildCommonArgs(config: BrowserLaunchConfig): string[] {
    const args = [
      `--remote-debugging-port=${config.port}`,
      '--disable-web-security',
      '--disable-features=TranslateUI',
      '--disable-component-update',
      '--disable-background-timer-throttling',
      '--disable-backgrounding-occluded-windows',
      '--disable-renderer-backgrounding',
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-default-apps',
      `--user-data-dir=${config.userDataDir}`
    ];

    // Add visibility and positioning
    if (config.headless && this.supportsHeadless) {
      args.push('--headless');
    }
    
    if (config.visible === false) {
      args.push('--window-position=-9999,-9999');
    } else if (config.position) {
      args.push(`--window-position=${config.position.x},${config.position.y}`);
    }
    
    if (config.size) {
      args.push(`--window-size=${config.size.width},${config.size.height}`);
    }
    
    if (config.minimized) {
      args.push('--start-minimized');
    }

    // Add custom args
    if (config.additionalArgs) {
      args.push(...config.additionalArgs);
    }

    return args;
  }

  /**
   * Abstract method - each browser implements its own launch args
   */
  abstract buildLaunchArgs(config: BrowserLaunchConfig): string[];

  /**
   * Utility: Check if file exists
   */
  private fileExists(path: string): boolean {
    try {
      require('fs').accessSync(path, require('fs').constants.F_OK);
      return true;
    } catch (error) {
      return false;
    }
  }
}