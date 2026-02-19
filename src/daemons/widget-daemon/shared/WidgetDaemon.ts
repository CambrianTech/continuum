/**
 * Widget Daemon - Shared Implementation
 * 
 * Simple bridge between widgets and JTAG command routing system.
 * Provides executeCommand() interface that widgets expect.
 */

import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import type { JTAGMessage, JTAGContext, CommandParams, CommandResult } from '../../../system/core/types/JTAGTypes';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { JTAGMessageFactory } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { SYSTEM_SCOPES } from '../../../system/core/types/SystemScopes';

export abstract class WidgetDaemon extends DaemonBase {
  public readonly subpath = 'widget';

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('widget-daemon', context, router);
  }

  /**
   * Execute command from widget - routes through JTAG CommandDaemon
   */
  async executeCommand(command: string, params: Omit<CommandParams, 'context' | 'sessionId' | 'userId'> = {}): Promise<CommandResult> {
    try {
      // Create command payload with session ID
      const payload = {
        ...params,
        context: this.context,
        sessionId: SYSTEM_SCOPES.SYSTEM, // Use system session for now
        userId: SYSTEM_SCOPES.SYSTEM
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
      const result = await this.router.postMessage(message);
      
      // Ensure the result has the required CommandResult fields
      return {
        context: this.context,
        sessionId: SYSTEM_SCOPES.SYSTEM,
        ...result
      } as CommandResult;
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`WidgetDaemon: Command ${command} failed:`, error);
      
      return {
        success: false,
        error: errorMessage,
        context: this.context,
        sessionId: SYSTEM_SCOPES.SYSTEM
      } as CommandResult;
    }
  }

  /**
   * Check if WidgetDaemon is connected to JTAG system
   */
  isConnected(): boolean {
    return this.router !== null;
  }

  /**
   * Process incoming JTAG messages (minimal - widgets don't receive messages)
   */
  protected async processMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    console.log(`WidgetDaemon: Received message to ${message.endpoint}`);
    return {
      success: true,
      timestamp: new Date().toISOString(),
      context: this.context,
      sessionId: SYSTEM_SCOPES.SYSTEM
    };
  }
}