/**
 * JTAG Connection Module
 * 
 * Handles connection management, health checks, and transport verification
 */

import { ContinuumConnectionParams, ContinuumConnection, DEFAULT_CONNECTION_PARAMS, JTAG_STATUS, JTAG_TRANSPORT, JTAGConfig } from '../JTAGTypes';

export class JTAGConnection {
  
  /**
   * Connect to JTAG system with health verification
   * Works on both client and server contexts
   */
  static async connect(
    config: JTAGConfig,
    isClient: boolean,
    isServer: boolean,
    sessionId: string,
    instanceUUID: string,
    webSocketServer: any,
    getWebSocketClient: () => Promise<any>,
    sendWebSocketMessage: (type: string, payload: any) => Promise<any>,
    emitStatusEvent: (status: any, transport?: any, details?: any) => void,
    startServerLogging: () => Promise<void>,
    log: (component: string, message: string, data?: any) => void,
    error: (component: string, message: string, data?: any) => void,
    params?: ContinuumConnectionParams
  ): Promise<ContinuumConnection> {
    
    // Merge with defaults
    const connectionConfig = { ...DEFAULT_CONNECTION_PARAMS, ...params };
    
    log('JTAG_CONNECT', 'Connection requested', { params: connectionConfig });
    
    const startTime = Date.now();
    let latency = 0;
    let transportType: 'websocket' | 'rest' | 'mcp' | 'polling' | 'sse' = 'websocket';
    let endpoint = '';
    let connectionState: 'connected' | 'connecting' | 'disconnected' | 'error' = 'connecting';
    
    try {
      // Start connection attempt
      emitStatusEvent(JTAG_STATUS.CONNECTING, JTAG_TRANSPORT.WEBSOCKET, {
        reason: 'connect_method_called',
        params: connectionConfig
      });
      
      if (connectionConfig.transport === 'auto' || connectionConfig.transport === 'websocket') {
        // Try WebSocket connection
        if (isClient) {
          const client = await getWebSocketClient();
          endpoint = `ws://localhost:${config.jtagPort || 9001}`;
          
          // Perform health check if requested
          if (connectionConfig.healthCheck) {
            const healthStart = Date.now();
            // Send ping message to verify connection health
            const healthResult = await sendWebSocketMessage('log', {
              component: 'HEALTH_CHECK',
              message: 'Connection health verification',
              type: 'test',
              timestamp: new Date().toISOString()
            });
            
            latency = Date.now() - healthStart;
            
            if (healthResult.success) {
              connectionState = 'connected';
              transportType = 'websocket';
              emitStatusEvent(JTAG_STATUS.READY, JTAG_TRANSPORT.WEBSOCKET, {
                reason: 'health_check_passed',
                latency,
                endpoint
              });
            } else {
              connectionState = 'error';
            }
          } else {
            // No health check - assume connected if WebSocket client exists
            connectionState = 'connected';
            latency = Date.now() - startTime;
            transportType = 'websocket';
          }
        } else {
          // Server-side - check if server is running
          endpoint = `ws://localhost:${config.jtagPort || 9001}`;
          
          if (webSocketServer) {
            connectionState = 'connected';
            transportType = 'websocket';
            latency = Date.now() - startTime;
            
            emitStatusEvent(JTAG_STATUS.READY, JTAG_TRANSPORT.WEBSOCKET, {
              reason: 'server_side_connection_verified',
              endpoint
            });
          } else {
            // Start server if not running
            await startServerLogging();
            connectionState = 'connected';
            transportType = 'websocket';
            latency = Date.now() - startTime;
          }
        }
      }
      
      // Build connection response
      const connection: ContinuumConnection = {
        healthy: connectionState === 'connected',
        transport: {
          type: transportType,
          state: connectionState,
          endpoint,
          latency
        },
        session: {
          id: sessionId,
          uuid: instanceUUID,
          uptime: Date.now() - startTime
        }
      };
      
      log('JTAG_CONNECT', 'Connection established', {
        healthy: connection.healthy,
        transport: connection.transport.type,
        latency: connection.transport.latency
      });
      
      return connection;
      
    } catch (err: any) {
      connectionState = 'error';
      
      emitStatusEvent(JTAG_STATUS.ERROR, JTAG_TRANSPORT.WEBSOCKET, {
        reason: 'connection_failed',
        error: err.message
      });
      
      error('JTAG_CONNECT', 'Connection failed', { error: err.message });
      
      // Return failed connection info
      return {
        healthy: false,
        transport: {
          type: transportType,
          state: 'error',
          endpoint: endpoint || 'unknown',
          latency: Date.now() - startTime
        },
        session: {
          id: sessionId,
          uuid: instanceUUID,
          uptime: 0
        }
      };
    }
  }
}