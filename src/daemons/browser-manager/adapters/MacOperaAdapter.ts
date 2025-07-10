/**
 * Mac Opera Adapter - Concrete implementation for Opera GX on macOS
 * 
 * Combines macOS AppleScript capabilities with Chromium-based browser features
 */

import { MacOSBrowserAdapter } from './os/MacOSBrowserAdapter.js';
import { ChromiumBasedAdapter } from './browser/ChromiumBasedAdapter.js';

export class MacOperaAdapter extends MacOSBrowserAdapter {
  private chromiumMixin: ChromiumBasedAdapter;
  
  constructor() {
    super('Opera GX');
    this.chromiumMixin = new ChromiumBasedAdapter();
  }
  
  protected getAppName(): string {
    return 'Opera GX';
  }
  
  // Override with Opera-specific logic if needed
  async countTabs(urlPattern: string): Promise<number> {
    // Use the parent AppleScript-based implementation
    return super.countTabs(urlPattern);
  }
  
  async closeTabs(urlPattern: string): Promise<number> {
    return super.closeTabs(urlPattern);
  }
  
  async focusTab(urlPattern: string): Promise<boolean> {
    return super.focusTab(urlPattern);
  }
  
  async refreshTab(urlPattern: string): Promise<boolean> {
    return super.refreshTab(urlPattern);
  }
  
  // Chromium-specific methods via mixin
  supportsDevTools(): boolean {
    return this.chromiumMixin.supportsDevTools();
  }
  
  supportsRemoteDebugging(): boolean {
    return this.chromiumMixin.supportsRemoteDebugging();
  }
  
  getCapabilities() {
    return {
      ...this.chromiumMixin.getCapabilities(),
      ...this.getAdapterInfo()
    };
  }
  
  // Opera-specific overrides
  async getBrowserVersion(): Promise<string | null> {
    try {
      // Opera might have different version format
      const version = await super.getBrowserVersion();
      return version?.replace('Opera ', '') || null;
    } catch {
      return null;
    }
  }
}