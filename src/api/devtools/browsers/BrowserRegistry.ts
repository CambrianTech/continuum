/**
 * Browser registry - plugin system for browser adapters
 * Auto-detects available browsers and provides fallback selection
 */

import { IBrowserAdapter, BrowserType } from './IBrowserAdapter.js';
import { ChromeBrowserAdapter } from './ChromeBrowserAdapter.js';
import { OperaBrowserAdapter, OperaGXBrowserAdapter } from './OperaBrowserAdapter.js';
import { FirefoxBrowserAdapter } from './FirefoxBrowserAdapter.js';

export class BrowserRegistry {
  private adapters = new Map<BrowserType, IBrowserAdapter>();
  private availableAdapters: IBrowserAdapter[] = [];

  constructor() {
    this.registerDefaultAdapters();
  }

  /**
   * Register all default browser adapters
   */
  private registerDefaultAdapters(): void {
    this.register(BrowserType.CHROME, new ChromeBrowserAdapter());
    this.register(BrowserType.OPERA, new OperaBrowserAdapter());
    this.register(BrowserType.OPERA_GX, new OperaGXBrowserAdapter());
    this.register(BrowserType.FIREFOX, new FirefoxBrowserAdapter());
  }

  /**
   * Register a browser adapter
   */
  register(type: BrowserType, adapter: IBrowserAdapter): void {
    this.adapters.set(type, adapter);
  }

  /**
   * Get specific browser adapter
   */
  get(type: BrowserType): IBrowserAdapter | null {
    return this.adapters.get(type) || null;
  }

  /**
   * Auto-detect available browsers on system
   */
  async detectAvailable(): Promise<IBrowserAdapter[]> {
    const available: IBrowserAdapter[] = [];
    
    for (const adapter of this.adapters.values()) {
      if (await adapter.isAvailable()) {
        available.push(adapter);
      }
    }
    
    this.availableAdapters = available;
    return available;
  }

  /**
   * Get best available browser for remote debugging
   */
  async getBestAvailable(): Promise<IBrowserAdapter | null> {
    if (this.availableAdapters.length === 0) {
      await this.detectAvailable();
    }

    // Priority order: Chrome > Opera GX > Opera > Firefox
    const priorities = [
      BrowserType.CHROME,
      BrowserType.OPERA_GX, 
      BrowserType.OPERA,
      BrowserType.FIREFOX
    ];

    for (const type of priorities) {
      const adapter = this.adapters.get(type);
      if (adapter && this.availableAdapters.includes(adapter)) {
        return adapter;
      }
    }

    // Return first available if no priority match
    return this.availableAdapters[0] || null;
  }

  /**
   * Get available browsers that support specific features
   */
  getByFeature(feature: 'headless' | 'remote-debugging'): IBrowserAdapter[] {
    return this.availableAdapters.filter(adapter => {
      switch (feature) {
        case 'headless':
          return adapter.supportsHeadless;
        case 'remote-debugging':
          return adapter.supportsRemoteDebugging;
        default:
          return true;
      }
    });
  }

  /**
   * List all available browsers with details
   */
  async listAvailable(): Promise<Array<{ type: BrowserType; name: string; available: boolean; path: string | null }>> {
    const result = [];
    
    for (const [type, adapter] of this.adapters.entries()) {
      const available = await adapter.isAvailable();
      const path = available ? adapter.getExecutablePath() : null;
      
      result.push({
        type,
        name: adapter.name,
        available,
        path
      });
    }
    
    return result;
  }
}

// Singleton registry
let registryInstance: BrowserRegistry | null = null;

export function getBrowserRegistry(): BrowserRegistry {
  if (!registryInstance) {
    registryInstance = new BrowserRegistry();
  }
  return registryInstance;
}