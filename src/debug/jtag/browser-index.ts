/**
 * JTAG Universal Command Bus - Browser Entry Point
 * 
 * Browser-specific entry point that exports JTAGSystemBrowser as jtag
 */

// Browser globals type declaration
declare const customElements: CustomElementRegistry;

import { JTAGSystemBrowser } from './system/core/system/browser/JTAGSystemBrowser';
import { JTAGClientBrowser } from './system/core/client/browser/JTAGClientBrowser';
import { createJTAGClientServices } from './system/core/client/shared/services';

// Import widget registry for dynamic registration
import { BROWSER_WIDGETS } from './browser/generated';

// Import widget debugging utilities
import * as WidgetUtils from './system/browser/WidgetUtils';

// NOTE: ThemeWidget imported via BROWSER_WIDGETS registry - no need for direct import

export const jtag = {
  // Widget debugging utilities
  widgets: WidgetUtils,
  
  // Universal client interface - always returns connection result with client property
  async connect(): Promise<ReturnType<typeof JTAGClientBrowser.connectLocal>> {
    console.debug('🔌 Browser: Connecting via JTAGClientBrowser (local connection)');

    // IMPORTANT: Connect to JTAG client FIRST before registering widgets
    // This ensures JTAGClient.sharedInstance is available when widgets initialize
    const connectionResult = await JTAGClientBrowser.connectLocal();
    const client = connectionResult.client;

    // Set up global window.jtag for widgets to access via JTAGClient.sharedInstance
    (globalThis as any).jtag = client;
    console.debug('🌐 Browser: Global jtag client established for widgets');

    // Enhance client with organized services - no globals needed
    const services = createJTAGClientServices(client);
    Object.assign(client, services);

    // NOW register widgets after JTAGClient.sharedInstance is available
    console.debug(`🎭 Registering ${BROWSER_WIDGETS.length} widgets...`);
    BROWSER_WIDGETS.forEach(widget => {
      if (!customElements.get(widget.tagName)) {
        try {
          customElements.define(widget.tagName, widget.widgetClass);
          console.debug(`✅ Registered widget: ${widget.tagName} (${widget.className})`);
        } catch (error) {
          console.warn(`⚠️ Failed to register widget ${widget.tagName}: ${error}`);
          console.warn(`⚠️ This usually means the constructor has already been used with this registry`);
          console.warn(`⚠️ Skipping registration for ${widget.tagName}`);
        }
      } else {
        console.warn(`⚠️ Widget ${widget.tagName} already registered, skipping`);
      }
    });

    console.debug('✅ Browser: JTAGClient connected with organized services');
    console.debug('ℹ️ Access services via: client.chat, client.users, client.widgets');

    return { ...connectionResult, client };
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