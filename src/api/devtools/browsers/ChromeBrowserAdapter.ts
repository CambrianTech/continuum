/**
 * Chrome browser adapter
 */

import { BaseBrowserAdapter } from './BaseBrowserAdapter.js';
import { BrowserLaunchConfig } from './IBrowserAdapter.js';

export class ChromeBrowserAdapter extends BaseBrowserAdapter {
  readonly name = 'Google Chrome';
  readonly supportsHeadless = true;
  readonly supportsRemoteDebugging = true;
  
  readonly executablePaths = [
    // macOS
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    '/Applications/Chrome.app/Contents/MacOS/Chrome',
    // Linux
    '/usr/bin/google-chrome',
    '/usr/bin/google-chrome-stable',
    '/usr/bin/chromium-browser',
    '/usr/bin/chromium',
    // Windows
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
    'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe'
  ];

  buildLaunchArgs(config: BrowserLaunchConfig): string[] {
    const args = this.buildCommonArgs(config);
    
    // Chrome-specific args
    args.push(
      '--disable-extensions',
      '--disable-plugins',
      '--disable-gpu-sandbox',
      `--app=${config.initialUrl}`,
      `--window-name=${config.windowTitle}`
    );

    return args;
  }
}