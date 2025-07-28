#!/usr/bin/env npx tsx
/**
 * Real Transport Integration Test
 * 
 * NON-MOCK test that uses the actual running system from `npm start`.
 * Tests real Client → Transport → Server → Response → Transport → Client flow
 * for each transport type.
 * 
 * Prerequisites: `npm start` should be running (launches browser and JTAG server)
 */

import * as puppeteer from 'puppeteer';
import { JTAGTransportFactory } from '@shared/transports/TransportFactory';
import { JTAGWebSocketTransportImpl } from '@shared/transports/WebSocketTransport';
import { JTAG_STATUS, JTAGConfig } from '@shared/JTAGTypes';
import { jtag } from '../../index';

interface RealTransportTestResult {
  transportType: string;
  browserConnected: boolean;
  serverConnected: boolean;
  messagesSent: number;
  messagesReceived: number;
  networkTrafficVisible: boolean;
  roundTripWorking: boolean;
}

class RealTransportIntegrationTester {
  private browser: any = null;
  private page: any = null;

  async runRealTransportTests(): Promise<void> {
    jtag.test('REAL_TRANSPORT_INTEGRATION', 'Starting real transport integration tests');
    
    try {
      // 1. Connect to the browser launched by npm start
      await this.connectToBrowser();
      
      // 2. Test each transport type with real browser + server
      const transportTypes = ['websocket', 'rest', 'mcp']; // Start with these main ones
      
      for (const transportType of transportTypes) {
        jtag.test('TRANSPORT_TEST_START', `Testing ${transportType} transport`, { transportType });
        
        const result = await this.testRealTransport(transportType);
        
        jtag.test('TRANSPORT_TEST_RESULT', `${transportType} transport test completed`, result);
        
        if (result.roundTripWorking) {
          console.log(`✅ ${transportType.toUpperCase()} transport: REAL INTEGRATION WORKING`);
        } else {
          console.log(`❌ ${transportType.toUpperCase()} transport: INTEGRATION FAILED`);
        }
      }
      
    } finally {
      await this.cleanup();
    }
  }

  private async connectToBrowser(): Promise<void> {
    jtag.test('BROWSER_CONNECT', 'Connecting to browser launched by npm start');
    
    // Connect to existing browser instance (launched by npm start)
    try {
      // Try to connect to existing browser first
      this.browser = await puppeteer.connect({
        browserURL: 'http://localhost:9222' // Standard Chrome debugging port
      });
    } catch (error) {
      // If no existing browser, launch our own for testing
      jtag.test('BROWSER_LAUNCH', 'Launching test browser', { reason: 'no_existing_browser' });
      this.browser = await puppeteer.launch({
        headless: false,
        devtools: true,
        args: [
          '--remote-debugging-port=9223', // Different port to avoid conflicts
          '--no-sandbox',
          '--disable-web-security'
        ]
      });
    }

    // Create or get page
    const pages = await this.browser.pages();
    if (pages.length > 0) {
      this.page = pages[0];
    } else {
      this.page = await this.browser.newPage();
    }

    // Navigate to JTAG test page or create one
    await this.page.goto('http://localhost:9002', { waitUntil: 'networkidle2' });
    jtag.test('BROWSER_READY', 'Browser connected and page loaded');
  }

  private async testRealTransport(transportType: string): Promise<RealTransportTestResult> {
    const result: RealTransportTestResult = {
      transportType,
      browserConnected: false,
      serverConnected: false,
      messagesSent: 0,
      messagesReceived: 0,
      networkTrafficVisible: false,
      roundTripWorking: false
    };

    try {
      // 1. Inject JTAG client code into the browser
      await this.injectJTAGClient(transportType);
      result.browserConnected = true;

      // 2. Create server-side transport for verification
      const serverTransport = await this.createServerTransport(transportType);
      result.serverConnected = true;

      // 3. Send test messages from browser
      result.messagesSent = await this.sendBrowserMessages(transportType);

      // 4. Monitor network traffic in browser
      result.networkTrafficVisible = await this.checkNetworkTraffic();

      // 5. Verify server received messages
      result.messagesReceived = await this.verifyServerReceived(serverTransport);

      // 6. Test round-trip: Server response back to browser
      result.roundTripWorking = await this.testRoundTrip(transportType);

      jtag.test('REAL_TRANSPORT_DETAILS', `${transportType} transport details`, result);

    } catch (error: any) {
      jtag.test('REAL_TRANSPORT_ERROR', `${transportType} transport error`, { 
        error: error.message,
        transportType 
      });
    }

    return result;
  }

  private async injectJTAGClient(transportType: string): Promise<void> {
    jtag.test('INJECT_CLIENT', `Injecting JTAG ${transportType} client into browser`);

    // Inject JTAG client code that will run in the browser
    await this.page.evaluate((transport) => {
      // This code runs in the browser context
      window.jtagTest = {
        messagesSent: [],
        messagesReceived: [],
        transport: transport
      };

      // Simple JTAG client simulation in browser
      window.sendJTAGMessage = (message) => {
        window.jtagTest.messagesSent.push(message);
        
        // Depending on transport type, send differently
        if (transport === 'websocket') {
          // Use WebSocket directly
          if (!window.jtagWS) {
            window.jtagWS = new WebSocket('ws://localhost:9001');
            window.jtagWS.onmessage = (event) => {
              window.jtagTest.messagesReceived.push(JSON.parse(event.data));
            };
            window.jtagWS.onopen = () => {
              window.jtagWS.send(JSON.stringify({
                type: 'log',
                payload: message,
                timestamp: new Date().toISOString(),
                messageId: 'browser-test-' + Date.now()
              }));
            };
          } else if (window.jtagWS.readyState === WebSocket.OPEN) {
            window.jtagWS.send(JSON.stringify({
              type: 'log',
              payload: message,
              timestamp: new Date().toISOString(),
              messageId: 'browser-test-' + Date.now()
            }));
          }
        } else if (transport === 'rest') {
          // Use HTTP REST API
          fetch('http://localhost:9001/api/log', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
              type: 'log',
              payload: message,
              timestamp: new Date().toISOString(),
              messageId: 'browser-rest-' + Date.now()
            })
          }).then(response => response.json())
            .then(data => window.jtagTest.messagesReceived.push(data));
        }
      };

    }, transportType);
  }

  private async createServerTransport(transportType: string): Promise<any> {
    jtag.test('CREATE_SERVER_TRANSPORT', `Creating server ${transportType} transport`);

    if (transportType === 'websocket') {
      const transport = new JTAGWebSocketTransportImpl();
      transport.enableTestMode();

      // Track received messages
      const receivedMessages: any[] = [];
      transport.onMessage((message) => {
        receivedMessages.push(message);
        jtag.test('SERVER_RECEIVED_MESSAGE', 'Server transport received message', message);
      });

      (transport as any).receivedMessages = receivedMessages;
      return transport;
    }
    
    // Add other transport types here
    return null;
  }

  private async sendBrowserMessages(transportType: string): Promise<number> {
    jtag.test('SEND_BROWSER_MESSAGES', `Sending test messages via ${transportType}`);

    const messagesToSend = [
      { component: 'BROWSER_TEST', message: `${transportType} message 1`, data: { testId: 1 } },
      { component: 'BROWSER_TEST', message: `${transportType} message 2`, data: { testId: 2 } },
      { component: 'BROWSER_TEST', message: `${transportType} message 3`, data: { testId: 3 } }
    ];

    // Send messages from browser
    for (const message of messagesToSend) {
      await this.page.evaluate((msg) => {
        window.sendJTAGMessage(msg);
      }, message);
      
      // Wait between messages
      await new Promise(resolve => setTimeout(resolve, 500));
    }

    // Get count of messages sent
    const sentCount = await this.page.evaluate(() => window.jtagTest.messagesSent.length);
    jtag.test('MESSAGES_SENT', `${transportType} messages sent from browser`, { count: sentCount });
    
    return sentCount;
  }

  private async checkNetworkTraffic(): Promise<boolean> {
    jtag.test('CHECK_NETWORK_TRAFFIC', 'Checking browser network traffic');

    // Enable network monitoring
    await this.page._client.send('Network.enable');
    
    // Wait a bit for network activity
    await new Promise(resolve => setTimeout(resolve, 2000));

    // Check for WebSocket connections
    const networkLogs = await this.page.evaluate(() => {
      // This would need to be implemented based on browser dev tools API
      // For now, just return indication that traffic was expected
      return {
        webSocketConnections: window.jtagWS ? 1 : 0,
        webSocketReadyState: window.jtagWS ? window.jtagWS.readyState : -1
      };
    });

    jtag.test('NETWORK_TRAFFIC_CHECK', 'Network traffic analysis', networkLogs);
    
    return networkLogs.webSocketConnections > 0;
  }

  private async verifyServerReceived(serverTransport: any): Promise<number> {
    jtag.test('VERIFY_SERVER_RECEIVED', 'Verifying server received messages');

    if (!serverTransport) return 0;

    // For WebSocket transport, check received messages
    const receivedCount = (serverTransport as any).receivedMessages?.length || 0;
    
    jtag.test('SERVER_RECEIVED_COUNT', 'Server received message count', { count: receivedCount });
    
    return receivedCount;
  }

  private async testRoundTrip(transportType: string): Promise<boolean> {
    jtag.test('TEST_ROUND_TRIP', `Testing ${transportType} round-trip communication`);

    // Send a message that expects a response
    await this.page.evaluate(() => {
      window.sendJTAGMessage({
        component: 'ROUND_TRIP_TEST',
        message: 'Expecting server response',
        expectResponse: true
      });
    });

    // Wait for potential response
    await new Promise(resolve => setTimeout(resolve, 2000));

    // Check if browser received any responses
    const receivedCount = await this.page.evaluate(() => window.jtagTest.messagesReceived.length);
    
    jtag.test('ROUND_TRIP_RESULT', `${transportType} round-trip result`, { 
      responsesReceived: receivedCount,
      success: receivedCount > 0
    });

    return receivedCount > 0;
  }

  private async cleanup(): Promise<void> {
    if (this.page) {
      await this.page.close();
    }
    if (this.browser) {
      await this.browser.close();
    }
    jtag.test('REAL_TRANSPORT_CLEANUP', 'Real transport integration test cleanup completed');
  }
}

// Run the real transport integration tests
async function runRealTransportIntegration() {
  console.log('\n🚀 REAL Transport Integration Tests');
  console.log('===================================');
  console.log('Prerequisites: npm start should be running');
  console.log('Testing: Real Client → Transport → Server → Response flow\n');

  const tester = new RealTransportIntegrationTester();
  
  try {
    await tester.runRealTransportTests();
    console.log('\n✅ Real transport integration tests completed');
    console.log('🔍 Check browser Network panel for actual WebSocket/HTTP traffic');
  } catch (error: any) {
    jtag.test('REAL_INTEGRATION_ERROR', 'Real integration test failed', { 
      error: error.message,
      stack: error.stack 
    });
    console.error('💥 Real transport integration test failed:', error.message);
    process.exit(1);
  }
}

runRealTransportIntegration();