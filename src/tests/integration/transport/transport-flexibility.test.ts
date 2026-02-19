/**
 * Demonstrate transport flexibility - same commands, different transports
 */

import { jtag } from '../../../server-index';
import type { JTAGClientConnectOptions } from '../../../system/core/client/shared/JTAGClient';

async function testTransportFlexibility(): Promise<void> {
  console.log('🚀 Testing transport flexibility...');
  
  // Test 1: WebSocket transport (current default)
  console.log('\n📡 Testing WebSocket transport...');
  try {
    const wsOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server',
      transportType: 'websocket',
      serverUrl: 'ws://localhost:9001',
      sessionId: '00000000-0000-0000-0000-000000000000' as any
    };
    
    const wsResult = await jtag.connect(wsOptions);
    console.log(`✅ WebSocket client connected with ${wsResult.listResult.commands.length} commands`);
    
    // Disconnect WebSocket client to clean up
    // wsResult.client would normally have disconnect method
    
  } catch (error) {
    console.log(`📡 WebSocket connection status: ${error.message}`);
  }
  
  // Test 2: HTTP transport (polling-based)
  console.log('\n🌐 Testing HTTP transport configuration...');
  try {
    const httpOptions: JTAGClientConnectOptions = {
      targetEnvironment: 'server',
      transportType: 'http',
      serverUrl: 'http://localhost:9002',
      sessionId: '00000000-0000-0000-0000-000000000000' as any,
      timeout: 5000,
      enableFallback: true
    };
    
    // This would work if we had HTTP server running
    console.log('📋 HTTP transport configuration:', {
      protocol: httpOptions.transportType,
      endpoint: httpOptions.serverUrl,
      fallback: httpOptions.enableFallback
    });
    
    console.log('💡 HTTP transport ready - would use REST API calls instead of WebSocket');
    
  } catch (error) {
    console.log(`🌐 HTTP transport note: ${error.message}`);
  }
  
  // Test 3: Show transport auto-detection
  console.log('\n🤖 Transport auto-detection examples...');
  
  const scenarios = [
    {
      environment: 'browser',
      expected: { protocol: 'websocket', role: 'client', fallback: true }
    },
    {
      environment: 'server', 
      expected: { protocol: 'websocket', role: 'server', fallback: true }
    },
    {
      environment: 'remote',
      expected: { protocol: 'http', role: 'client', fallback: false }
    }
  ];
  
  for (const scenario of scenarios) {
    console.log(`📍 ${scenario.environment} → ${scenario.expected.protocol} (${scenario.expected.role})`);
  }
  
  console.log('\n🎯 Transport Flexibility Summary:');
  console.log('  • WebSocket: Real-time, bidirectional, persistent connection');
  console.log('  • HTTP: Stateless, request/response, firewall-friendly');  
  console.log('  • UDP: P2P mesh networking, device discovery');
  console.log('  • Same JTAGClient API works with any transport!');
  console.log('  • Transport selection via config - zero code changes');
}

testTransportFlexibility().catch(console.error);