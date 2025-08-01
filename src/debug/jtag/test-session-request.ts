#!/usr/bin/env tsx
/**
 * Test Session Request - Proper session flow via SessionDaemon
 */

import { WebSocketTransportServerClient } from './system/transports/websocket-transport/server/WebSocketTransportServerClient';
import type { JTAGMessage, JTAGRequestMessage, JTAGPayload, JTAGContext } from './shared/JTAGTypes';
import { SYSTEM_SCOPES } from './shared/SystemScopes';
import { generateUUID } from './shared/CrossPlatformUUID';
import { JTAG_ENDPOINTS } from './shared/JTAGEndpoints';

class SessionRequestHandler {
  private responses = new Map<string, JTAGMessage>();
  public realSessionId: string | null = null;

  async handleTransportMessage(message: JTAGMessage): Promise<void> {
    console.log(`🔔 CLIENT RECEIVED MESSAGE!`);
    console.log(`📨 Received: ${message.messageType || (message as any).type} to ${message.endpoint || 'no-endpoint'}`);
    console.log(`📨 Message correlationId: ${(message as any).correlationId?.slice(0, 12) || 'none'}...`);
    console.log(`📨 Full message:`, JSON.stringify(message, null, 2));
    
    if ((message.messageType === 'response' || (message as any).type === 'response') && (message as any).correlationId) {
      const correlationId = (message as any).correlationId;
      this.responses.set(correlationId, message);
      console.log(`✅ Got response for correlation: ${correlationId.slice(0, 12)}...`);
      console.log(`✅ Waiting for correlations: ${Array.from(this.responses.keys()).map(k => k.slice(0, 8)).join(', ')}`);
    }
  }

  async sendAndWait(transport: WebSocketTransportServerClient, endpoint: string, params: any = {}, sessionId?: string): Promise<any> {
    const correlationId = generateUUID();
    const useSessionId = sessionId || SYSTEM_SCOPES.UNKNOWN_SESSION;
    
    // Create proper context and payload with strict typing
    const context: JTAGContext = {
      uuid: useSessionId as any, // UUID type
      environment: 'server'
    };
    
    const payload: JTAGPayload & typeof params = {
      ...params,
      context, // REQUIRED: JTAGContext
      sessionId: useSessionId as any // REQUIRED: UUID
    };
    
    const message: JTAGRequestMessage = {
      messageType: 'request',
      context,
      origin: 'client',
      endpoint,
      payload,
      correlationId,
      hashCode: () => correlationId // Required method
    };
    
    console.log(`📤 Sending to ${endpoint} with correlation ${correlationId.slice(0, 12)}...`);
    await transport.send(message);
    
    // Wait for response
    for (let i = 0; i < 50; i++) {
      await new Promise(resolve => setTimeout(resolve, 100));
      if (this.responses.has(correlationId)) {
        const response = this.responses.get(correlationId)!;
        console.log(`🎯 Found matching response for ${correlationId.slice(0, 12)}!`);
        return response.payload;
      }
      
      // Debug: show what correlations we DO have
      if (i === 10 && this.responses.size > 0) {
        console.log(`🔍 Debug: Looking for ${correlationId.slice(0, 8)}, but have: ${Array.from(this.responses.keys()).map(k => k.slice(0, 8)).join(', ')}`);
      }
    }
    
    throw new Error(`Request to ${endpoint} timeout`);
  }
}

async function testSessionFlow() {
  try {
    console.log('🧪 Testing proper session flow via SessionDaemon...');
    
    const handler = new SessionRequestHandler();
    const transport = new WebSocketTransportServerClient({
      url: 'ws://localhost:9001',
      handler: handler as any,
      sessionHandshake: true
    });
    
    await transport.connect('ws://localhost:9001');
    console.log('✅ Connected with bootstrap session');
    
    // Wait for connection to stabilize
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // Step 1: Request default session from SessionDaemon using proper typed endpoints
    console.log('\n🤝 Step 1: Requesting default session from SessionDaemon...');
    try {
      const sessionResult = await handler.sendAndWait(transport, JTAG_ENDPOINTS.SESSION.GET_DEFAULT);
      handler.realSessionId = sessionResult.sessionId;
      console.log(`✅ Got real session ID: ${handler.realSessionId?.slice(0, 12)}...`);
    } catch (error) {
      console.log(`⚠️ Session request failed: ${error.message}`);
      console.log('💡 Trying alternative session endpoints...');
      
      // Try alternative session endpoints using typed constants
      try {
        const sessionResult = await handler.sendAndWait(transport, JTAG_ENDPOINTS.SESSION.GET_CURRENT);
        handler.realSessionId = sessionResult.sessionId;
        console.log(`✅ Got session via session/current: ${handler.realSessionId?.slice(0, 12)}...`);
      } catch (error2) {
        console.log(`⚠️ Alternative failed too: ${error2.message}`);
        console.log('🔄 Using bootstrap session for now...');
      }
    }
    
    // Step 2: Test LIST command with proper session
    console.log('\n📋 Step 2: Testing LIST command with proper session...');
    const sessionToUse = handler.realSessionId || SYSTEM_SCOPES.UNKNOWN_SESSION;
    
    const listResult = await handler.sendAndWait(transport, 'commands/list', { category: 'all' }, sessionToUse);
    console.log(`✅ LIST worked with session! Found ${listResult.commands?.length || 0} commands`);

    // Step 3: Test health/ping command
    console.log('\n💓 Step 3: Testing HEALTH/PING command...');
    try {
      const pingResult = await handler.sendAndWait(transport, 'health', {}, sessionToUse);
      console.log(`✅ HEALTH command successful:`, pingResult);
    } catch (error) {
      console.log(`❌ HEALTH command failed:`, error.message);
    }

    // Step 4: Test screenshot command (if available)
    console.log('\n📸 Step 4: Testing SCREENSHOT command...');
    try {
      const screenshotResult = await handler.sendAndWait(transport, 'screenshot', { 
        querySelector: 'body',
        filename: 'test-ping-screenshot.png'
      }, sessionToUse);
      console.log(`✅ SCREENSHOT command successful:`, screenshotResult);
    } catch (error) {
      console.log(`❌ SCREENSHOT command failed:`, error.message);
    }
    
    console.log('\n🎉 Session flow test completed!');
    
    await transport.disconnect();
    
  } catch (error) {
    console.error('❌ Session flow test failed:', error);
  }
}

testSessionFlow();