/**
 * JTAGClientFactory Integration Test
 * 
 * Tests the smart getClient() functionality to ensure it connects to the running system
 * and provides a fully functional client.
 */

import { strict as assert } from 'assert';

async function testJTAGClientFactory() {
  console.log('🧪 Testing JTAGClientFactory.getClient()...');
  console.log('🔌 Environment:', typeof window === 'undefined' ? 'Server (Node.js)' : 'Browser');
  
  try {
    // Import the main JTAG entry point
    const { jtag } = await import('../../index');
    
    // Test the smart getClient method
    console.log('📞 Calling jtag.getClient()...');
    const client = await jtag.getClient();
    
    // Validate client structure
    assert(typeof client === 'object', 'getClient() should return client object');
    assert(typeof client.sessionId === 'string', 'Client should have session ID');
    assert(typeof client.commands === 'object', 'Client should have commands interface');
    
    console.log('✅ Client obtained:', {
      sessionId: client.sessionId,
      hasCommands: typeof client.commands === 'object',
      commandCount: client.commands ? Object.keys(client.commands).length : 0
    });
    
    // Test list command (single dependency pattern)
    if (client.commands && client.commands.list) {
      console.log('📋 Testing list command...');
      const listResult = await client.commands.list();
      
      assert(listResult && listResult.commands, 'List command should return commands array');
      assert(Array.isArray(listResult.commands), 'Commands should be an array');
      assert(listResult.commands.length > 0, 'Should discover available commands');
      
      console.log('✅ List command successful:', {
        commandCount: listResult.commands.length,
        firstFew: listResult.commands.slice(0, 3)
      });
    } else {
      throw new Error('List command not available - single dependency pattern broken');
    }
    
    // Test screenshot command if available
    if (client.commands && client.commands.screenshot) {
      console.log('📸 Testing screenshot command...');
      const screenshotResult = await client.commands.screenshot({
        querySelector: 'body',
        filename: 'jtag-client-factory-test.png'
      });
      
      assert(screenshotResult, 'Screenshot command should return result');
      console.log('✅ Screenshot command successful:', screenshotResult);
    }
    
    console.log('🎉 All tests passed! JTAGClientFactory.getClient() is working perfectly.');
    return true;
    
  } catch (error) {
    console.error('❌ Test failed:', error.message);
    if (error.stack) {
      console.error('Stack:', error.stack);
    }
    return false;
  }
}

// Export for test runner
export { testJTAGClientFactory };

// Run if called directly
if (require.main === module) {
  testJTAGClientFactory()
    .then(success => {
      process.exit(success ? 0 : 1);
    })
    .catch(error => {
      console.error('💥 Unexpected error:', error);
      process.exit(1);
    });
}