/**
 * Core WebSocket Handlers
 * 
 * Essential handlers that register themselves with MessageRouter.
 * This demonstrates proper dependency inversion - handlers know about
 * MessageRouter, not the other way around.
 */

import { messageRouter } from '../core/MessageRouter';
import { DaemonConnector } from '../core/DaemonConnector';
import { CommandRequest } from '../types';

/**
 * Register all core handlers with the MessageRouter
 */
export function registerCoreHandlers(): void {
  console.log('🔧 Registering core WebSocket handlers');

  // Ping/Pong handler
  messageRouter.registerHandler('ping', async (_data: any, clientId: string) => {
    return {
      pong: true,
      timestamp: new Date().toISOString(),
      clientId
    };
  });

  // Server stats handler
  messageRouter.registerHandler('get_stats', async (_data: any, _clientId: string, daemonConnector: DaemonConnector) => {
    return {
      server: 'websocket-server',
      version: '1.0.0',
      uptime: process.uptime(),
      memory: process.memoryUsage(),
      connections: 1,
      daemon_connected: daemonConnector.isConnected(),
      registered_handlers: messageRouter.getRegisteredTypes()
    };
  });

  // Client initialization handler
  messageRouter.registerHandler('client_init', async (_data: any, clientId: string, daemonConnector: DaemonConnector) => {
    console.log(`🚀 Client initialization: ${clientId}`);
    
    return {
      initialized: true,
      server_time: new Date().toISOString(),
      client_id: clientId,
      server_info: {
        name: 'Continuum WebSocket Server',
        version: '1.0.0',
        supported_operations: messageRouter.getRegisteredTypes()
      },
      daemon_status: {
        connected: daemonConnector.isConnected(),
        type: 'typescript'
      }
    };
  });

  // Command execution handler
  messageRouter.registerHandler('execute_command', async (data: CommandRequest, clientId: string, daemonConnector: DaemonConnector) => {
    console.log(`🔍 Executing command: ${data.command} from ${clientId}`);
    
    if (!daemonConnector.isConnected()) {
      return {
        command: data.command,
        result: {
          success: false,
          error: 'TypeScript daemon not connected'
        }
      };
    }

    try {
      const result = await daemonConnector.executeCommand(
        data.command,
        parseParams(data.params),
        { clientId }
      );
      
      return {
        command: data.command,
        result: result || { success: true, data: 'Command completed' },
        requestId: data.requestId
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        command: data.command,
        result: {
          success: false,
          error: errorMessage
        }
      };
    }
  });

  console.log(`✅ Registered ${messageRouter.getHandlerCount()} core handlers`);
}

/**
 * Helper function to parse command parameters
 */
function parseParams(params: string): any {
  try {
    return JSON.parse(params);
  } catch {
    return {};
  }
}