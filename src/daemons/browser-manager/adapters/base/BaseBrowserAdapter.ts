/**
 * Base Browser Adapter - Universal interface for browser tab management
 * 
 * This defines the contract that all browser adapters must implement,
 * regardless of OS or browser type.
 */

export interface TabCloseResult {
  browser: string;
  closed: number;
  error?: string;
}

export abstract class BaseBrowserAdapter {
  constructor(
    protected browserName: string,
    protected osName: string
  ) {}
  
  // Core browser operations - must be implemented by concrete adapters
  abstract countTabs(urlPattern: string): Promise<number>;
  abstract closeTabs(urlPattern: string): Promise<number>;
  abstract focusTab(urlPattern: string): Promise<boolean>;
  
  // Browser identification
  abstract isAvailable(): Promise<boolean>;
  abstract getBrowserVersion(): Promise<string | null>;
  
  // Utility methods available to all adapters
  protected extractDomain(url: string): string {
    try {
      return new URL(url).hostname;
    } catch {
      return url;
    }
  }
  
  protected isRelativeImport(importPath: string): boolean {
    return importPath.startsWith('./') || importPath.startsWith('../');
  }
  
  protected hasFileExtension(importPath: string): boolean {
    return /\.[a-zA-Z0-9]+$/.test(importPath);
  }
  
  // Adapter metadata
  getBrowserName(): string {
    return this.browserName;
  }
  
  getOSName(): string {
    return this.osName;
  }
  
  getAdapterInfo(): { browser: string; os: string; type: string } {
    return {
      browser: this.browserName,
      os: this.osName,
      type: this.constructor.name
    };
  }
}