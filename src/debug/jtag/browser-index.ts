/**
 * JTAG Universal Command Bus - Browser Entry Point
 * 
 * Browser-specific entry point that exports JTAGSystemBrowser as jtag
 */

import { JTAGSystemBrowser } from './system/core/system/browser/JTAGSystemBrowser';
import { JTAGClientBrowser } from './system/core/client/browser/JTAGClientBrowser';

// Import widget registry for dynamic registration
import { BROWSER_WIDGETS } from './browser/generated';

export const jtag = {
  // Universal client interface - always returns connection result with client property
  async connect(): Promise<ReturnType<typeof JTAGClientBrowser.connectLocal>> {
    console.log('🔌 Browser: Connecting via JTAGClientBrowser (local connection)');
    
    // Register widgets dynamically from generated registry
    console.log(`🎭 Registering ${BROWSER_WIDGETS.length} widgets...`);
    BROWSER_WIDGETS.forEach(widget => {
      if (!customElements.get(widget.tagName)) {
        customElements.define(widget.tagName, widget.widgetClass);
        console.log(`✅ Registered widget: ${widget.tagName} (${widget.className})`);
      }
    });
    
    const connectionResult = await JTAGClientBrowser.connectLocal();
    console.log('✅ Browser: JTAGClient connected with local system');
    return connectionResult;
  },

  // Legacy: Full system access (for advanced usage)
  async getSystem(): Promise<ReturnType<typeof JTAGSystemBrowser.connect>> {
    return JTAGSystemBrowser.connect();
  }
};

// Direct export for advanced usage
export { JTAGSystemBrowser };
export * from './system/core/types/JTAGTypes';
export * from './commands/screenshot/shared/browser-utils/BrowserElementUtils';