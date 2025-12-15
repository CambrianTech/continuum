/**
 * JTAG Universal Command Bus - Browser Entry Point
 * 
 * Browser-specific entry point that exports JTAGSystemBrowser as jtag
 */

// Browser globals type declaration
declare const customElements: CustomElementRegistry;

import { JTAGSystemBrowser } from './system/core/system/browser/JTAGSystemBrowser';
import { JTAGClientBrowser } from './system/core/client/browser/JTAGClientBrowser';
import { JTAGClient } from './system/core/client/shared/JTAGClient';
import { createJTAGClientServices } from './system/core/client/shared/services';

// Import widget registry for dynamic registration
import { BROWSER_WIDGETS } from './browser/generated';

// Import widget debugging utilities
import * as WidgetUtils from './system/browser/WidgetUtils';

// Import WidgetDiscovery for universal selector support
import { WidgetDiscovery } from './system/core/browser/utils/WidgetIntrospection';

// NOTE: ThemeWidget imported via BROWSER_WIDGETS registry - no need for direct import

export const jtag = {
  // Widget debugging utilities
  widgets: WidgetUtils,
  
  // Universal client interface - always returns connection result with client property
  async connect(): Promise<ReturnType<typeof JTAGClientBrowser.connectLocal>> {
    // Reduce log spam
    // console.debug('🔌 Browser: Connecting via JTAGClientBrowser (local connection)');

    // Connect client FIRST
    const connectionResult = await JTAGClientBrowser.connectLocal();
    const client = connectionResult.client;

    // Set up global window.jtag for widgets and tests
    (globalThis as any).jtag = client;

    // Expose WidgetDiscovery for universal selector support (used by GlobalUtils.safeQuerySelector)
    (globalThis as any).WidgetDiscovery = WidgetDiscovery;

    // Register client in static registry for sharedInstance access
    JTAGClient.registerClient('default', client);

    // NOW register widgets - after client is available for sharedInstance
    // console.debug(`🎭 Registering ${BROWSER_WIDGETS.length} widgets...`);
    BROWSER_WIDGETS.forEach(widget => {
      if (!customElements.get(widget.tagName)) {
        try {
          customElements.define(widget.tagName, widget.widgetClass);
          // console.debug(`✅ Registered widget: ${widget.tagName} (${widget.className})`);
        } catch (error) {
          console.warn(`⚠️ Failed to register widget ${widget.tagName}: ${error}`);
          console.warn(`⚠️ This usually means the constructor has already been used with this registry`);
          console.warn(`⚠️ Skipping registration for ${widget.tagName}`);
        }
      } else {
        console.warn(`⚠️ Widget ${widget.tagName} already registered, skipping`);
      }
    });

    // Enhance client with organized services - no globals needed
    const services = createJTAGClientServices(client);
    Object.assign(client, services);




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
export * from './commands/interface/screenshot/shared/browser-utils/BrowserElementUtils';