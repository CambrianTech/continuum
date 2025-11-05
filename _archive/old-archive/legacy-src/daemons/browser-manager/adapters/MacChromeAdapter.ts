/**
 * Mac Chrome Adapter - Concrete implementation for Google Chrome on macOS
 * 
 * Combines macOS AppleScript capabilities with Chromium-based browser features
 */

import { MacOSBrowserAdapter } from './os/MacOSBrowserAdapter.js';
import { ChromiumBasedAdapter } from './browser/ChromiumBasedAdapter.js';

export class MacChromeAdapter extends MacOSBrowserAdapter {
  private chromiumMixin: ChromiumBasedAdapter;
  
  constructor() {
    super('Google Chrome');
    this.chromiumMixin = new ChromiumBasedAdapter();
  }
  
  protected getAppName(): string {
    return 'Google Chrome';
  }
  
  // Use parent AppleScript-based implementations
  async countTabs(urlPattern: string): Promise<number> {
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
}