/**
 * Opera browser adapter (including Opera GX)
 */

import { BaseBrowserAdapter } from './BaseBrowserAdapter.js';
import { BrowserLaunchConfig } from './IBrowserAdapter.js';

export class OperaBrowserAdapter extends BaseBrowserAdapter {
  readonly name = 'Opera';
  readonly supportsHeadless = true;
  readonly supportsRemoteDebugging = true;
  
  readonly executablePaths = [
    // macOS
    '/Applications/Opera.app/Contents/MacOS/Opera',
    '/Applications/Opera GX.app/Contents/MacOS/Opera',
    // Linux  
    '/usr/bin/opera',
    '/usr/bin/opera-stable',
    // Windows
    'C:\\Program Files\\Opera\\launcher.exe',
    'C:\\Program Files (x86)\\Opera\\launcher.exe',
    'C:\\Users\\%USERNAME%\\AppData\\Local\\Programs\\Opera GX\\launcher.exe'
  ];

  buildLaunchArgs(config: BrowserLaunchConfig): string[] {
    const args = this.buildCommonArgs(config);
    
    // Opera-specific args
    args.push(
      '--disable-extensions',
      `--app=${config.initialUrl}`,
      `--window-name=${config.windowTitle}`
    );

    return args;
  }
}

export class OperaGXBrowserAdapter extends OperaBrowserAdapter {
  readonly name = 'Opera GX';
  
  readonly executablePaths = [
    // macOS
    '/Applications/Opera GX.app/Contents/MacOS/Opera',
    // Windows
    'C:\\Users\\%USERNAME%\\AppData\\Local\\Programs\\Opera GX\\launcher.exe',
    // Fallback to regular Opera
    ...super.executablePaths
  ];
}