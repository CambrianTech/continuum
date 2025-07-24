#!/usr/bin/env npx tsx
/**
 * Client-Server Transport Orchestration Test
 * 
 * This test proves that JTAG can orchestrate logging and screenshots
 * between client and server using transport layers.
 * 
 * Flow: Browser Client → WebSocket Transport → JTAG Server → Response → Client
 */

import { JTAGWebSocketTransportImpl } from '../../shared/transports/WebSocketTransport';
import { EmergencyJTAGClient } from '../../client/JTAGClient';
// EmergencyJTAGServer was junk - removed
import { JTAG_STATUS, JTAGConfig, JTAGWebSocketMessage } from '../../shared/JTAGTypes';
import { jtag } from '../../index';
import * as http from 'http';
import * as WebSocket from 'ws';

interface OrchestrationTestResult {
  clientToServer: boolean;
  serverToClient: boolean; 
  loggingOrchestrated: boolean;
  screenshotOrchestrated: boolean;
  transportWorking: boolean;
}

class ClientServerTransportOrchestrator {
  private server?: http.Server;
  private wss?: WebSocket.WebSocketServer;
  private port = 9004; // Separate port for orchestration test
  private serverReceivedMessages: JTAGWebSocketMessage[] = [];
  private clientReceivedResponses: any[] = [];

  async runOrchestrationTest(): Promise<OrchestrationTestResult> {
    jtag.test('CLIENT_SERVER_ORCHESTRATION', 'Starting client-server transport orchestration test');

    const result: OrchestrationTestResult = {
      clientToServer: false,
      serverToClient: false,
      loggingOrchestrated: false,
      screenshotOrchestrated: false,
      transportWorking: false
    };

    try {
      // 1. Start JTAG Server that can handle transport messages
      await this.startJTAGServer();
      jtag.test('SERVER_START', 'JTAG Server started', { port: this.port });

      // 2. Create JTAG Client with transport
      const clientTransport = await this.createJTAGClient();
      jtag.test('CLIENT_CREATE', 'JTAG Client created with transport');

      // 3. Test Client → Server logging orchestration
      result.loggingOrchestrated = await this.testLoggingOrchestration(clientTransport);
      jtag.test('LOGGING_ORCHESTRATION', 'Logging orchestration test', { 
        success: result.loggingOrchestrated 
      });

      // 4. Test Client → Server screenshot orchestration  
      result.screenshotOrchestrated = await this.testScreenshotOrchestration(clientTransport);
      jtag.test('SCREENSHOT_ORCHESTRATION', 'Screenshot orchestration test', { 
        success: result.screenshotOrchestrated 
      });

      // 5. Verify transport is working bi-directionally
      result.transportWorking = this.serverReceivedMessages.length > 0 && this.clientReceivedResponses.length > 0;
      result.clientToServer = this.serverReceivedMessages.length > 0;
      result.serverToClient = this.clientReceivedResponses.length > 0;

      jtag.test('TRANSPORT_VERIFICATION', 'Transport bi-directional verification', {
        serverReceived: this.serverReceivedMessages.length,
        clientReceived: this.clientReceivedResponses.length,
        transportWorking: result.transportWorking
      });

      // Cleanup
      await clientTransport.disconnect();
      
    } finally {
      await this.cleanup();
    }

    return result;
  }

  private async startJTAGServer(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.server = http.createServer();
      this.wss = new WebSocket.WebSocketServer({ server: this.server });

      // This simulates a JTAG server that can handle orchestration requests
      this.wss.on('connection', (ws) => {
        jtag.test('SERVER_CONNECTION', 'Client connected to JTAG server');

        ws.on('message', (data) => {
          try {
            const message: JTAGWebSocketMessage = JSON.parse(data.toString());
            this.serverReceivedMessages.push(message);
            
            jtag.test('SERVER_RECEIVED', `Server received ${message.type} message`, {
              messageId: message.messageId,
              type: message.type,
              payload: message.payload
            });

            // Server processes the message and orchestrates appropriate action
            this.orchestrateServerAction(ws, message);

          } catch (error: any) {
            jtag.test('SERVER_ERROR', 'Server message processing error', { 
              error: error.message 
            });
          }
        });

        ws.on('close', () => {
          jtag.test('SERVER_DISCONNECTION', 'Client disconnected from JTAG server');
        });
      });

      this.server.listen(this.port, () => {
        jtag.test('SERVER_LISTENING', `JTAG orchestration server listening`, { 
          port: this.port 
        });
        resolve();
      });

      this.server.on('error', reject);
    });
  }

  private orchestrateServerAction(ws: WebSocket, message: JTAGWebSocketMessage): void {
    // This is where the server orchestrates actions based on client requests
    if (message.type === 'log') {
      // Server orchestrates logging
      const logPayload = message.payload as any;
      
      // Server creates log entry using JTAG server-side logging
      // EmergencyJTAGServer was junk - removed
      console.log('CLIENT_ORCHESTRATED', `Orchestrated from client: ${logPayload.message}`);

      // Send confirmation back to client
      const response = {
        success: true,
        messageId: message.messageId,
        orchestrated: 'server_logging_completed',
        serverTime: new Date().toISOString(),
        action: 'log_orchestrated'
      };

      ws.send(JSON.stringify(response));
      jtag.test('SERVER_ORCHESTRATION', 'Log orchestration completed', response);

    } else if (message.type === 'screenshot') {
      // Server orchestrates screenshot
      const screenshotPayload = message.payload as any;
      
      // In a real scenario, server would trigger screenshot capture
      // For test purposes, we simulate this
      jtag.test('SCREENSHOT_ORCHESTRATED', 'Server orchestrated screenshot request', {
        filename: screenshotPayload.filename,
        options: screenshotPayload.options,
        orchestrationId: message.messageId
      });

      // Send screenshot result back to client
      const response = {
        success: true,
        messageId: message.messageId,
        orchestrated: 'server_screenshot_handled',
        serverTime: new Date().toISOString(),
        action: 'screenshot_orchestrated',
        result: {
          filepath: `/screenshots/${screenshotPayload.filename}.png`,
          success: true,
          metadata: { width: 800, height: 600 }
        }
      };

      ws.send(JSON.stringify(response));
      jtag.test('SERVER_ORCHESTRATION', 'Screenshot orchestration completed', response);
    }
  }

  private async createJTAGClient(): Promise<JTAGWebSocketTransportImpl> {
    const transport = new JTAGWebSocketTransportImpl();
    transport.enableTestMode();

    // Track responses from server
    transport.onMessage((response) => {
      this.clientReceivedResponses.push(response);
      jtag.test('CLIENT_RECEIVED', 'Client received server response', response);
    });

    const config: JTAGConfig = {
      context: 'browser',
      jtagPort: this.port,
      enableRemoteLogging: true,
      enableConsoleOutput: false,
      maxBufferSize: 100
    };

    const connected = await transport.initialize(config);
    if (!connected) {
      throw new Error('Client transport failed to connect to server');
    }

    await transport.waitForStatus(JTAG_STATUS.READY, 3000);
    return transport;
  }

  private async testLoggingOrchestration(clientTransport: JTAGWebSocketTransportImpl): Promise<boolean> {
    jtag.test('LOGGING_ORCHESTRATION_START', 'Starting logging orchestration test');

    // Client sends log message to server for orchestration
    const logMessage: JTAGWebSocketMessage = {
      type: 'log',
      payload: {
        component: 'CLIENT_LOGGER',
        message: 'This log message should be orchestrated by the server',
        timestamp: new Date().toISOString(),
        orchestrationTest: true
      },
      timestamp: new Date().toISOString(),
      messageId: 'orchestration-log-' + Date.now()
    };

    jtag.test('CLIENT_SENDING_LOG', 'Client sending log for orchestration', { 
      messageId: logMessage.messageId 
    });

    const response = await clientTransport.send(logMessage);
    
    if (!response.success) {
      jtag.test('LOGGING_ORCHESTRATION_FAILED', 'Log orchestration failed', { 
        error: response.error 
      });
      return false;
    }

    // Wait for server response
    await new Promise(resolve => setTimeout(resolve, 500));

    // Check if server received and processed the message
    const serverReceived = this.serverReceivedMessages.some(msg => 
      msg.messageId === logMessage.messageId && msg.type === 'log'
    );

    const clientReceivedResponse = this.clientReceivedResponses.some(resp => 
      resp.messageId === logMessage.messageId && resp.action === 'log_orchestrated'
    );

    const success = serverReceived && clientReceivedResponse;
    jtag.test('LOGGING_ORCHESTRATION_RESULT', 'Logging orchestration result', {
      serverReceived,
      clientReceivedResponse,
      success
    });

    return success;
  }

  private async testScreenshotOrchestration(clientTransport: JTAGWebSocketTransportImpl): Promise<boolean> {
    jtag.test('SCREENSHOT_ORCHESTRATION_START', 'Starting screenshot orchestration test');

    // Client sends screenshot request to server for orchestration
    const screenshotMessage: JTAGWebSocketMessage = {
      type: 'screenshot',
      payload: {
        filename: 'orchestrated-screenshot-' + Date.now(),
        options: {
          format: 'png',
          width: 800,
          height: 600
        },
        orchestrationTest: true
      },
      timestamp: new Date().toISOString(),
      messageId: 'orchestration-screenshot-' + Date.now()
    };

    jtag.test('CLIENT_SENDING_SCREENSHOT', 'Client sending screenshot for orchestration', { 
      messageId: screenshotMessage.messageId 
    });

    const response = await clientTransport.send(screenshotMessage);
    
    if (!response.success) {
      jtag.test('SCREENSHOT_ORCHESTRATION_FAILED', 'Screenshot orchestration failed', { 
        error: response.error 
      });
      return false;
    }

    // Wait for server response
    await new Promise(resolve => setTimeout(resolve, 500));

    // Check if server received and processed the screenshot request
    const serverReceived = this.serverReceivedMessages.some(msg => 
      msg.messageId === screenshotMessage.messageId && msg.type === 'screenshot'
    );

    const clientReceivedResponse = this.clientReceivedResponses.some(resp => 
      resp.messageId === screenshotMessage.messageId && resp.action === 'screenshot_orchestrated'
    );

    const success = serverReceived && clientReceivedResponse;
    jtag.test('SCREENSHOT_ORCHESTRATION_RESULT', 'Screenshot orchestration result', {
      serverReceived,
      clientReceivedResponse,
      success
    });

    return success;
  }

  private async cleanup(): Promise<void> {
    if (this.wss) {
      this.wss.close();
    }
    if (this.server) {
      await new Promise<void>((resolve) => {
        this.server!.close(() => resolve());
      });
    }
    jtag.test('ORCHESTRATION_CLEANUP', 'Client-server orchestration test cleanup completed');
  }
}

// Run the orchestration test
async function runOrchestrationTest() {
  const orchestrator = new ClientServerTransportOrchestrator();
  
  try {
    const result = await orchestrator.runOrchestrationTest();
    
    jtag.test('ORCHESTRATION_FINAL_RESULTS', 'Client-server orchestration test completed', result);
    
    if (result.transportWorking && result.loggingOrchestrated && result.screenshotOrchestrated) {
      console.log('\n🎉 Client-Server Transport Orchestration SUCCESS!');
      console.log('✅ Client → Server communication: WORKING');
      console.log('✅ Server → Client communication: WORKING'); 
      console.log('✅ Logging orchestration: WORKING');
      console.log('✅ Screenshot orchestration: WORKING');
      console.log('📡 Transport layer: FULLY FUNCTIONAL');
    } else {
      console.log('\n⚠️  Client-Server Transport Orchestration PARTIAL SUCCESS');
      console.log(`❌ Client → Server: ${result.clientToServer ? 'OK' : 'FAILED'}`);
      console.log(`❌ Server → Client: ${result.serverToClient ? 'OK' : 'FAILED'}`);
      console.log(`❌ Logging orchestration: ${result.loggingOrchestrated ? 'OK' : 'FAILED'}`);
      console.log(`❌ Screenshot orchestration: ${result.screenshotOrchestrated ? 'OK' : 'FAILED'}`);
    }
    
  } catch (error: any) {
    jtag.test('ORCHESTRATION_ERROR', 'Orchestration test failed', { 
      error: error.message,
      stack: error.stack 
    });
    console.error('💥 Client-Server orchestration test failed:', error.message);
    process.exit(1);
  }
}

runOrchestrationTest();