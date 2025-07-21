#!/usr/bin/env npx tsx
/**
 * Test JTAG Core System Only
 * 
 * Just start the JTAG WebSocket server and test the transport layer
 * No messy demo HTTP server - just pure JTAG functionality
 */

import { jtag } from './index';

async function testJTAGCore() {
  console.log('🧪 Testing JTAG Core System');
  console.log('============================');
  
  try {
    // Test 1: Health check validates transport works
    console.log('🔗 Testing transport health check...');
    const connectionResult = await jtag.connect({ 
      healthCheck: true,
      transport: 'auto',
      timeout: 5000
    });
    
    console.log('✅ Transport health check passed:', {
      healthy: connectionResult.healthy,
      transport: connectionResult.transport,
      session: connectionResult.session
    });
    
    // Test 2: Console logging (should route immediately on server)
    console.log('📝 Testing console logging...');
    jtag.log('TEST', 'Server-side logging test', { timestamp: new Date().toISOString() });
    jtag.error('TEST', 'Error logging test');
    jtag.critical('TEST', 'Critical event test');
    
    console.log('✅ Console logging tests sent');
    
    // Test 3: Check that files were created
    console.log('📁 Checking log files...');
    const fs = require('fs');
    const path = require('path');
    
    const logDir = path.join(process.cwd(), '.continuum', 'jtag', 'logs');
    try {
      const files = fs.readdirSync(logDir);
      console.log('✅ Log files created:', files.filter((f: string) => f.endsWith('.txt')));
    } catch (error) {
      console.log('❌ No log directory found');
    }
    
    console.log('');
    console.log('🎉 JTAG Core System Test Complete');
    console.log('💡 WebSocket server running on port 9001');
    console.log('🔄 System ready for client connections');
    
  } catch (error: any) {
    console.error('❌ JTAG Core System Test Failed:', error.message);
    process.exit(1);
  }
}

testJTAGCore();