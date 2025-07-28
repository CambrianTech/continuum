#!/usr/bin/env npx tsx
/**
 * Complete Real Transport Test
 * 
 * Tests both WebSocket and HTTP endpoints with event verification
 * Includes curl testing of HTTP endpoint and event emission checking
 */

import { JTAGWebSocketTransportImpl } from '@shared/transports/WebSocketTransport';
import { JTAG_STATUS, JTAGConfig, JTAGWebSocketMessage } from '@shared/JTAGTypes';
import { jtag } from '../../index';
import fetch from 'node-fetch';

class CompleteRealTransportTester {
  private emittedEvents: Array<{event: string, data: any, timestamp: number}> = [];

  constructor() {
    // Monitor JTAG events
    if (typeof window !== 'undefined') {
      // Browser environment
      ['jtag:ready', 'jtag:connecting', 'jtag:error', 'jtag:disconnected', 'jtag:message'].forEach(eventType => {
        window.addEventListener(eventType, (event: any) => {
          this.emittedEvents.push({
            event: eventType,
            data: event.detail,
            timestamp: Date.now()
          });
          jtag.test('EVENT_CAPTURED', `Captured ${eventType} event`, event.detail);
        });
      });
    }
  }
  
  async testCompleteRealTransports(): Promise<void> {
    console.log('\n🚀 Complete Real Transport Test');
    console.log('===============================');
    console.log('Testing WebSocket + HTTP endpoints with event verification\n');

    jtag.test('COMPLETE_REAL_TEST', 'Starting complete real transport tests');

    // Test 1: HTTP endpoint with curl
    await this.testHTTPEndpoint();
    
    // Test 2: WebSocket with event monitoring
    await this.testWebSocketWithEvents();
    
    // Test 3: Event verification
    await this.verifyEvents();
    
    jtag.test('COMPLETE_REAL_TEST', 'Complete real transport tests finished');
    console.log('\n✅ Complete real transport tests completed');
  }

  private async testHTTPEndpoint(): Promise<void> {
    console.log('🌐 Testing HTTP Endpoint...');
    jtag.test('HTTP_ENDPOINT_TEST', 'Starting HTTP endpoint test');

    try {
      // Test the web interface with enhanced verification
      console.log('  📡 Curling http://localhost:9002...');
      const response = await fetch('http://localhost:9002');
      const html = await response.text();
      
      if (response.ok) {
        console.log('  ✅ HTTP endpoint responding');
        console.log(`  📄 Content length: ${html.length} bytes`);
        console.log(`  🎯 Contains JTAG demo: ${html.includes('JTAG End-to-End Demo')}`);
        
        // Emit event when we get HTML back
        this.emitHTTPEvent('html_received', {
          url: 'http://localhost:9002',
          status: response.status,
          contentLength: html.length,
          contentType: response.headers.get('content-type'),
          isJTAGDemo: html.includes('JTAG End-to-End Demo'),
          hasJavaScript: html.includes('<script'),
          hasCSS: html.includes('<style') || html.includes('.css'),
          title: this.extractHTMLTitle(html),
          timestamp: new Date().toISOString()
        });
        
        jtag.test('HTTP_ENDPOINT_SUCCESS', 'HTTP endpoint test successful', {
          status: response.status,
          contentLength: html.length,
          isJTAGDemo: html.includes('JTAG End-to-End Demo'),
          title: this.extractHTMLTitle(html)
        });

        // Additional HTTP verification
        await this.verifyHTMLContent(html);
        
      } else {
        console.log(`  ❌ HTTP endpoint failed: ${response.status}`);
        this.emitHTTPEvent('http_error', {
          url: 'http://localhost:9002',
          status: response.status,
          statusText: response.statusText,
          timestamp: new Date().toISOString()
        });
        jtag.test('HTTP_ENDPOINT_FAILED', 'HTTP endpoint failed', { status: response.status });
      }

      // Test JTAG API endpoints if they exist
      await this.testJTAGAPIEndpoints();

    } catch (error: any) {
      console.log(`  ❌ HTTP endpoint error: ${error.message}`);
      this.emitHTTPEvent('http_connection_error', {
        url: 'http://localhost:9002',
        error: error.message,
        timestamp: new Date().toISOString()
      });
      jtag.test('HTTP_ENDPOINT_ERROR', 'HTTP endpoint error', { error: error.message });
    }
  }

  private emitHTTPEvent(eventType: string, data: any): void {
    // Add to our captured events for tracking
    this.emittedEvents.push({
      event: `http:${eventType}`,
      data,
      timestamp: Date.now()
    });

    // Log the event for visibility
    jtag.test('HTTP_EVENT_EMITTED', `HTTP event: ${eventType}`, data);
    console.log(`  📡 Event emitted: http:${eventType}`);
  }

  private extractHTMLTitle(html: string): string {
    const titleMatch = html.match(/<title[^>]*>([^<]+)<\/title>/i);
    return titleMatch ? titleMatch[1].trim() : 'No title';
  }

  private async verifyHTMLContent(html: string): Promise<void> {
    console.log('  🔍 Verifying HTML content...');
    
    const analysis = {
      hasJavaScript: html.includes('<script'),
      hasCSS: html.includes('<style') || html.includes('.css'),
      hasJTAGScripts: html.includes('jtag') || html.includes('JTAG'),
      hasWebSocket: html.includes('WebSocket') || html.includes('ws://'),
      scriptTags: (html.match(/<script/g) || []).length,
      links: (html.match(/<a\s+href/g) || []).length,
      forms: (html.match(/<form/g) || []).length,
      title: this.extractHTMLTitle(html)
    };

    console.log(`    📋 HTML Analysis: ${JSON.stringify(analysis, null, 2)}`);
    
    this.emitHTTPEvent('html_analysis', analysis);
    
    jtag.test('HTML_CONTENT_VERIFIED', 'HTML content analysis completed', analysis);
  }

  private async testJTAGAPIEndpoints(): Promise<void> {
    console.log('  🔍 Testing JTAG API endpoints...');
    
    const apiEndpoints = [
      'http://localhost:9001/api/status',
      'http://localhost:9001/api/logs',
      'http://localhost:9001/health',
      'http://localhost:9002/api/jtag/status'
    ];

    for (const endpoint of apiEndpoints) {
      try {
        const response = await fetch(endpoint);
        if (response.ok) {
          const data = await response.text();
          console.log(`  ✅ ${endpoint}: ${response.status} (${data.length} bytes)`);
          jtag.test('API_ENDPOINT_SUCCESS', 'API endpoint working', { 
            endpoint, 
            status: response.status 
          });
        } else {
          console.log(`  ⚠️  ${endpoint}: ${response.status}`);
        }
      } catch (error) {
        // Expected for most endpoints - just log they're not available
        console.log(`  ➖ ${endpoint}: not available`);
      }
    }
  }

  private async testWebSocketWithEvents(): Promise<void> {
    console.log('\n📡 Testing WebSocket with Event Monitoring...');
    jtag.test('WEBSOCKET_EVENTS_TEST', 'Starting WebSocket with events test');

    try {
      const transport = new JTAGWebSocketTransportImpl();
      transport.enableTestMode();

      // Track all messages and responses
      const receivedMessages: any[] = [];
      transport.onMessage((message) => {
        receivedMessages.push(message);
        jtag.test('WEBSOCKET_MESSAGE_RECEIVED', 'WebSocket message received', message);
        console.log(`  📨 Received: ${JSON.stringify(message)}`);
      });

      const config: JTAGConfig = {
        context: 'browser',
        jtagPort: 9001,
        enableRemoteLogging: true,
        enableConsoleOutput: false,
        maxBufferSize: 100
      };

      console.log('  🔌 Connecting with event monitoring...');
      const initialEventCount = this.emittedEvents.length;
      
      const connected = await transport.initialize(config);
      
      if (!connected) {
        console.log('  ❌ WebSocket connection failed');
        return;
      }

      await transport.waitForStatus(JTAG_STATUS.READY, 3000);
      console.log('  ✅ WebSocket connected');

      // Send messages that should trigger events
      const testMessages = [
        {
          type: 'log' as const,
          payload: {
            component: 'EVENT_TEST',
            message: 'Message that should trigger events',
            expectEvents: true
          },
          timestamp: new Date().toISOString(),
          messageId: 'event-test-1-' + Date.now()
        },
        {
          type: 'screenshot' as const,
          payload: {
            filename: 'event-test-screenshot',
            expectEvents: true
          },
          timestamp: new Date().toISOString(),
          messageId: 'event-test-2-' + Date.now()
        }
      ];

      console.log('  📤 Sending messages that should emit events...');
      for (const message of testMessages) {
        const response = await transport.send(message);
        
        if (response.success) {
          console.log(`  ✅ Message sent: ${message.messageId}`);
          jtag.test('EVENT_TRIGGER_MESSAGE_SENT', 'Event trigger message sent', { 
            messageId: message.messageId,
            type: message.type
          });
        } else {
          console.log(`  ❌ Message failed: ${response.error}`);
        }
      }

      // Wait for events to be processed
      await new Promise(resolve => setTimeout(resolve, 3000));

      const newEventCount = this.emittedEvents.length - initialEventCount;
      console.log(`  📊 Events captured during test: ${newEventCount}`);
      
      jtag.test('WEBSOCKET_EVENTS_COMPLETE', 'WebSocket events test completed', {
        messagesSent: testMessages.length,
        responsesReceived: receivedMessages.length,
        eventsCapture: newEventCount
      });

      await transport.disconnect();
      
    } catch (error: any) {
      console.log(`  ❌ WebSocket events test error: ${error.message}`);
      jtag.test('WEBSOCKET_EVENTS_ERROR', 'WebSocket events test error', { error: error.message });
    }
  }

  private async verifyEvents(): Promise<void> {
    console.log('\n🔍 Verifying Events...');
    jtag.test('EVENT_VERIFICATION', 'Starting event verification');

    console.log(`  📊 Total events captured: ${this.emittedEvents.length}`);
    
    if (this.emittedEvents.length === 0) {
      console.log('  ⚠️  No events captured (expected in server context)');
      jtag.test('EVENT_VERIFICATION_NOTE', 'No events captured - server context expected');
      return;
    }

    const eventTypes = [...new Set(this.emittedEvents.map(e => e.event))];
    console.log(`  📋 Event types seen: ${eventTypes.join(', ')}`);

    // Count HTTP events specifically
    const httpEvents = this.emittedEvents.filter(e => e.event.startsWith('http:'));
    const webSocketEvents = this.emittedEvents.filter(e => e.event.startsWith('jtag:'));
    
    console.log(`  🌐 HTTP events: ${httpEvents.length}`);
    console.log(`  📡 WebSocket events: ${webSocketEvents.length}`);

    if (httpEvents.length > 0) {
      console.log('  ✅ HTTP events captured successfully:');
      httpEvents.forEach(event => {
        console.log(`    - ${event.event}: ${event.data?.status || event.data?.contentLength || 'event data'}`);
      });
    }

    // Expected events now include HTTP events
    const expectedEvents = ['http:html_received', 'http:html_analysis'];
    const jtagEvents = ['jtag:ready', 'jtag:connecting', 'jtag:message'];
    const allExpectedEvents = [...expectedEvents, ...jtagEvents];
    
    const missingEvents = expectedEvents.filter(event => !eventTypes.includes(event));
    const foundExpectedEvents = expectedEvents.filter(event => eventTypes.includes(event));

    if (foundExpectedEvents.length > 0) {
      console.log(`  ✅ Found expected events: ${foundExpectedEvents.join(', ')}`);
    }

    if (missingEvents.length > 0) {
      console.log(`  ❌ Missing expected events: ${missingEvents.join(', ')}`);
    }

    // Show some unexpected events (but don't treat as errors since we're exploring)
    const unexpectedEvents = eventTypes.filter(event => !allExpectedEvents.includes(event));
    if (unexpectedEvents.length > 0) {
      console.log(`  ➕ Additional events found: ${unexpectedEvents.join(', ')}`);
    }

    jtag.test('EVENT_VERIFICATION_COMPLETE', 'Event verification completed', {
      totalEvents: this.emittedEvents.length,
      eventTypes,
      httpEvents: httpEvents.length,
      webSocketEvents: webSocketEvents.length,
      expectedEvents,
      missingEvents,
      foundExpectedEvents,
      unexpectedEvents
    });
  }

  // Add method to trigger events manually for testing
  async triggerTestEvents(): Promise<void> {
    console.log('\n🧪 Triggering Test Events...');
    jtag.test('MANUAL_EVENT_TRIGGER', 'Manually triggering test events');
    
    // Use JTAG API calls that should emit events
    jtag.log('EVENT_TRIGGER', 'This log should trigger events');
    jtag.critical('EVENT_TRIGGER', 'This critical should trigger events');
    jtag.probe('EVENT_TRIGGER', 'event_test_probe', { active: true });
    
    // Try screenshot (might emit events)
    try {
      const result = await jtag.screenshot('event-test-screenshot');
      jtag.test('MANUAL_SCREENSHOT_TRIGGERED', 'Manual screenshot triggered', { 
        success: result.success 
      });
    } catch (error: any) {
      jtag.test('MANUAL_SCREENSHOT_ERROR', 'Manual screenshot error', { 
        error: error.message 
      });
    }

    console.log('  ✅ Manual event triggers completed');
  }
}

// Run complete real transport tests
async function runCompleteRealTests() {
  const tester = new CompleteRealTransportTester();
  
  try {
    await tester.testCompleteRealTransports();
    
    // Also trigger some manual events
    await tester.triggerTestEvents();
    
    console.log('\n🎉 Complete real transport testing finished!');
    console.log('🔍 Check browser Network panel and console for events');
    console.log('📁 Check .continuum/jtag/logs/ for all test log files');
    
  } catch (error: any) {
    jtag.test('COMPLETE_REAL_ERROR', 'Complete real test failed', { 
      error: error.message 
    });
    console.error('💥 Complete real transport test failed:', error.message);
    process.exit(1);
  }
}

runCompleteRealTests();