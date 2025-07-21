#!/usr/bin/env npx tsx
/**
 * Real Network Traffic Test
 * 
 * Simple test: just log stuff with JTAG and see if it shows up in browser Network panel
 * No fancy simulations - just use the actual JTAG system.
 */

import { jtag } from '../../index';

// Just log some test messages and see if they go through the real transport
async function testRealNetworkTraffic() {
  console.log('🧪 Testing real JTAG network traffic...');
  console.log('🔍 Check browser Network panel for WebSocket traffic on port 9001');
  
  // Test the new test log level
  jtag.test('NETWORK_TEST', 'Real network traffic test starting');
  
  // Send various log messages that should go through WebSocket
  jtag.log('REAL_TRANSPORT_TEST', 'This should appear in Network panel');
  jtag.critical('REAL_TRANSPORT_TEST', 'Critical message via real transport');
  jtag.probe('REAL_TRANSPORT_TEST', 'network_probe', { connected: true, port: 9001 });
  
  // Console messages should also be intercepted and sent
  console.log('🚨 This console.log should go through JTAG WebSocket!');
  console.error('🚨 This console.error should go through JTAG WebSocket!');
  
  // Test multiple messages quickly
  for (let i = 0; i < 5; i++) {
    jtag.test('RAPID_TEST', `Rapid test message ${i + 1}`, { index: i, timestamp: Date.now() });
  }
  
  jtag.test('NETWORK_TEST', 'Real network traffic test completed');
  
  console.log('✅ Test completed - check browser Network panel for WebSocket messages');
  console.log('📁 Also check .continuum/jtag/logs/server.test.txt for test log entries');
}

testRealNetworkTraffic();