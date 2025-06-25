/**
 * Firefox browser adapter
 */

import { BaseBrowserAdapter } from './BaseBrowserAdapter.js';
import { BrowserLaunchConfig } from './IBrowserAdapter.js';

export class FirefoxBrowserAdapter extends BaseBrowserAdapter {
  readonly name = 'Mozilla Firefox';
  readonly supportsHeadless = true;
  readonly supportsRemoteDebugging = true;
  
  readonly executablePaths = [
    // macOS
    '/Applications/Firefox.app/Contents/MacOS/firefox',
    '/Applications/Firefox Developer Edition.app/Contents/MacOS/firefox',
    // Linux
    '/usr/bin/firefox',
    '/usr/bin/firefox-esr',
    // Windows
    'C:\\Program Files\\Mozilla Firefox\\firefox.exe',
    'C:\\Program Files (x86)\\Mozilla Firefox\\firefox.exe'
  ];

  buildLaunchArgs(config: BrowserLaunchConfig): string[] {
    // Firefox uses different remote debugging approach
    const args = [
      '--remote-debugging-port=' + config.port,
      '--no-first-run',
      '--no-default-browser-check',
      `--profile=${config.userDataDir}`,
      config.initialUrl
    ];

    // Firefox-specific visibility args
    if (config.headless) {
      args.push('--headless');
    }
    
    if (config.visible === false) {
      args.push('--width=1', '--height=1');
    } else if (config.size) {
      args.push(`--width=${config.size.width}`, `--height=${config.size.height}`);
    }

    if (config.additionalArgs) {
      args.push(...config.additionalArgs);
    }

    return args;
  }

  /**
   * Firefox uses different DevTools endpoint
   */
  getDevToolsEndpoint(port: number): string {
    return `http://localhost:${port}/json/list`;
  }
}