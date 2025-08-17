#!/usr/bin/env tsx
/**
 * EventsDaemon Unit Test
 * 
 * Tests EventsDaemon event emission and bridging independent of chat system
 */

import { EventsDaemonServer } from '../../daemons/events-daemon/server/EventsDaemonServer';
import { EventsDaemonBrowser } from '../../daemons/events-daemon/browser/EventsDaemonBrowser';
import { JTAGMessageFactory } from '../../system/core/types/JTAGTypes';
import type { JTAGContext } from '../../system/core/types/JTAGTypes';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import { EVENT_ENDPOINTS } from '../../daemons/events-daemon/shared/EventEndpoints';

// Mock router for testing
class MockRouter {
  public async postMessage(message: any) {
    console.log('📤 MockRouter: Posted message:', message.endpoint);
    return { success: true };
  }
  
  public async routeToServer() { return { success: true }; }
  public async routeToBrowser() { return { success: true }; }
}

async function testEventsDaemonUnit() {
  console.log('🧪 UNIT TEST: EventsDaemon emission and bridging...');
  
  try {
    // Create test context
    const context: JTAGContext = {
      uuid: generateUUID(),
      environment: 'server'
    };
    
    const mockRouter = new MockRouter() as any;
    
    // Test server EventsDaemon
    console.log('🖥️ Testing EventsDaemonServer...');
    const serverDaemon = new EventsDaemonServer(context, mockRouter);
    
    // Create test event message
    const eventPayload = {
      eventName: 'test-event',
      data: { message: 'Unit test event' },
      scope: { type: 'system' },
      originSessionId: context.uuid,
      timestamp: new Date().toISOString()
    };
    
    const eventMessage = JTAGMessageFactory.createEvent(
      context,
      'unit-test',
      `events/${EVENT_ENDPOINTS.BRIDGE}`,
      eventPayload as any
    );
    
    console.log('📨 Sending event to server daemon...');
    console.log('🔍 Event message:', JSON.stringify(eventMessage, null, 2));
    
    const result = await serverDaemon.handleMessage(eventMessage);
    console.log('🔍 Server result:', JSON.stringify(result, null, 2));
    
    if (result.success && result.bridged) {
      console.log('✅ UNIT TEST PASSED: EventsDaemonServer correctly processes events');
    } else {
      console.log('❌ UNIT TEST FAILED: EventsDaemonServer failed to process event');
      console.log('❌ Expected: success=true, bridged=true');
      console.log('❌ Actual:', { success: result.success, bridged: result.bridged });
      return false;
    }
    
    // Test browser EventsDaemon (simulated DOM environment)
    console.log('🌐 Testing EventsDaemonBrowser...');
    
    // Mock DOM for browser test
    global.document = {
      createElement: () => ({ style: {}, textContent: '', appendChild: () => {} }),
      body: { appendChild: () => {} },
      dispatchEvent: (event: any) => {
        console.log(`✨ DOM EVENT DISPATCHED: ${event.type}`);
        return true;
      },
      addEventListener: () => {}
    } as any;
    
    global.CustomEvent = class MockCustomEvent {
      type: string;
      detail: any;
      
      constructor(type: string, options: any) {
        this.type = type;
        this.detail = options.detail;
      }
    } as any;
    
    const browserContext: JTAGContext = { ...context, environment: 'browser' };
    const browserDaemon = new EventsDaemonBrowser(browserContext, mockRouter);
    
    console.log('📨 Sending event to browser daemon...');
    const browserResult = await browserDaemon.handleMessage({
      ...eventMessage,
      context: browserContext
    });
    
    if (browserResult.success && browserResult.bridged) {
      console.log('✅ UNIT TEST PASSED: EventsDaemonBrowser correctly processes and bridges events');
    } else {
      console.log('❌ UNIT TEST FAILED: EventsDaemonBrowser failed to process event');
      return false;
    }
    
    console.log('🎉 UNIT TEST SUITE PASSED: EventsDaemon works correctly in isolation');
    return true;
    
  } catch (error) {
    console.error('❌ UNIT TEST FAILED:', error);
    return false;
  }
}

// Run test
testEventsDaemonUnit().then((success) => {
  if (success) {
    console.log('✅ EventsDaemon unit tests completed successfully');
    process.exit(0);
  } else {
    console.log('💥 EventsDaemon unit tests failed');
    process.exit(1);
  }
}).catch((error) => {
  console.error('💥 Unit test crashed:', error);
  process.exit(1);
});