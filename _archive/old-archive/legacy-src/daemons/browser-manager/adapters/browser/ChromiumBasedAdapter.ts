/**
 * Chromium-Based Browser Adapter - Shared logic for Chrome, Opera, Edge, etc.
 * 
 * This mixin/interface provides common functionality for all Chromium-based browsers
 * including DevTools Protocol support, debugging capabilities, etc.
 */

export interface ChromiumCapabilities {
  supportsDevTools: boolean;
  supportsRemoteDebugging: boolean;
  defaultDebugPort: number;
  supportedExtensions: string[];
}

export class ChromiumBasedAdapter {
  protected capabilities: ChromiumCapabilities = {
    supportsDevTools: true,
    supportsRemoteDebugging: true,
    defaultDebugPort: 9222,
    supportedExtensions: ['.crx']
  };
  
  // Chromium-specific URL pattern matching
  protected isValidContinuumURL(url: string, pattern: string): boolean {
    // Chromium browsers handle URLs consistently
    return (
      url === pattern ||
      url === pattern + '/' ||
      url.startsWith(pattern + '?') ||
      url.startsWith(pattern + '#')
    );
  }
  
  // DevTools Protocol utilities (for future use)
  protected getDebugPort(): number {
    return this.capabilities.defaultDebugPort;
  }
  
  protected getDevToolsURL(port: number): string {
    return `http://localhost:${port}`;
  }
  
  // Chromium-specific browser detection
  protected getChromiumUserDataDir(): string {
    const os = process.platform;
    switch (os) {
      case 'darwin':
        return '~/Library/Application Support';
      case 'linux':
        return '~/.config';
      case 'win32':
        return '%LOCALAPPDATA%';
      default:
        return '~/.config';
    }
  }
  
  // Common Chromium command line arguments
  protected getCommonChromiumArgs(): string[] {
    return [
      '--no-first-run',
      '--no-default-browser-check',
      '--disable-backgrounding-occluded-windows',
      '--disable-renderer-backgrounding',
      '--disable-features=TranslateUI',
      '--disable-ipc-flooding-protection'
    ];
  }
  
  protected getDebugArgs(port: number): string[] {
    return [
      `--remote-debugging-port=${port}`,
      '--remote-debugging-address=127.0.0.1'
    ];
  }
  
  // Chromium capabilities
  getCapabilities(): ChromiumCapabilities {
    return { ...this.capabilities };
  }
  
  supportsDevTools(): boolean {
    return this.capabilities.supportsDevTools;
  }
  
  supportsRemoteDebugging(): boolean {
    return this.capabilities.supportsRemoteDebugging;
  }
}