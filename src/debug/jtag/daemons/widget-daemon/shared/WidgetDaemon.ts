/**
 * Widget Daemon - Shared Implementation
 * 
 * Simple bridge between widgets and JTAG command routing system.
 * Provides executeCommand() interface that widgets expect.
 */

import { DaemonBase } from '@shared/DaemonBase';
import type { JTAGMessage, JTAGContext } from '@shared/JTAGTypes';
import { JTAGMessageFactory } from '@shared/JTAGTypes';
import type { JTAGRouter } from '@shared/JTAGRouter';
import { SYSTEM_SCOPES } from '@shared/SystemScopes';

export abstract class WidgetDaemon extends DaemonBase {
  public readonly subpath = 'widget';

  constructor(name: string, context: JTAGContext, router: JTAGRouter) {
    super(name, context, router);
  }

  /**
   * Execute command from widget - routes through JTAG CommandDaemon
   */
  async executeCommand(command: string, params: any = {}): Promise<any> {
    try {
      // Create command payload with session ID
      const payload = {
        ...params,
        context: this.context,
        sessionId: SYSTEM_SCOPES.SYSTEM // Use system session for now
      };

      // Create JTAG message for command routing
      const message = JTAGMessageFactory.createRequest(
        this.context,
        this.subpath,
        `commands/${command}`,
        payload,
        `widget_${Date.now()}`
      );

      // Route through JTAG router to CommandDaemon
      return await this.router.postMessage(message);
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`WidgetDaemon: Command ${command} failed:`, error);
      
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Check if WidgetDaemon is connected to JTAG system
   */
  isConnected(): boolean {
    return this.router !== null;
  }

  /**
   * Handle incoming JTAG messages (minimal - widgets don't receive messages)
   */
  async handleMessage(message: JTAGMessage): Promise<any> {
    console.log(`WidgetDaemon: Received message to ${message.endpoint}`);
    return { success: true, message: 'WidgetDaemon received message' };
  }
}