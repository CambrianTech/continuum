/**
 * Widget Daemon - Browser Implementation
 * 
 * Browser-specific widget daemon that provides global access for widgets.
 */

import { WidgetDaemon } from '@daemonsWidgetDaemon/shared/WidgetDaemon';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { JTAGRouter } from '@shared/JTAGRouter';

export class WidgetDaemonBrowser extends WidgetDaemon {

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Initialize widget daemon - make it globally available
   */
  protected async initialize(): Promise<void> {
    console.log('✅ WidgetDaemonBrowser: Connected to JTAG system via router');
    
    // Make this daemon globally available for widgets
    (window as any).widgetDaemon = this;
  }

  /**
   * Cleanup on shutdown
   */
  async shutdown(): Promise<void> {
    // Remove global reference
    if ((window as any).widgetDaemon === this) {
      delete (window as any).widgetDaemon;
    }

    await super.shutdown();
  }
}