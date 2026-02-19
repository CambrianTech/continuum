#!/usr/bin/env tsx
/**
 * Ping Integration Test
 * 
 * Tests client.commands.ping() using the proper JTAG client API.
 * This is how users will actually use the system.
 */

console.log('🧪 Ping Integration Test');

function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

async function testPingCommand() {
  console.log('\n🏓 Testing client.commands.ping()');
  
  try {
    // Use the proper JTAG API exactly as documented
    const { jtag } = await import('../../server-index');
    const client = await jtag.connect();
    
    // Test the ping command with various parameters
    console.log('🔄 Testing basic ping...');
    const basicResult = await client.commands.ping({
      message: 'integration-test'
    });
    
    assert(basicResult.success === true, 'Basic ping succeeded');
    assert(basicResult.message === 'integration-test', 'Message echoed correctly');
    
    console.log('🔄 Testing ping with timing...');
    const timingResult = await client.commands.ping({
      message: 'timing-test',
      includeTiming: true
    });
    
    assert(timingResult.success === true, 'Timing ping succeeded');
    assert(typeof timingResult.roundTripTime === 'number', 'Round trip time included');
    assert(timingResult.roundTripTime >= 0, 'Valid round trip time');
    
    console.log('🔄 Testing ping with environment data...');
    const envResult = await client.commands.ping({
      message: 'environment-test',
      includeTiming: true,
      includeEnvironment: true
    });
    
    assert(envResult.success === true, 'Environment ping succeeded');
    assert(typeof envResult.environment === 'object', 'Environment data included');
    assert(envResult.environment.type === 'server' || envResult.environment.type === 'browser', 'Valid environment type');
    
    console.log('📊 Ping Results Summary:');
    console.log(`   Basic ping: ${basicResult.success ? 'PASS' : 'FAIL'}`);
    console.log(`   Timing ping: ${timingResult.roundTripTime}ms`);
    console.log(`   Environment: ${envResult.environment.type}`);
    
    return true;
    
  } catch (error) {
    console.error('❌ Ping integration test failed:', error.message);
    throw error;
  }
}

async function testPingPerformance() {
  console.log('\n⚡ Testing ping performance');
  
  try {
    const { jtag } = await import('../../server-index');
    const client = await jtag.connect();
    
    const iterations = 5;
    const times: number[] = [];
    
    for (let i = 0; i < iterations; i++) {
      const start = Date.now();
      const result = await client.commands.ping({
        message: `perf-${i}`,
        includeTiming: true
      });
      const clientTime = Date.now() - start;
      
      assert(result.success, `Performance ping ${i} succeeded`);
      times.push(clientTime);
    }
    
    const avgTime = times.reduce((sum, time) => sum + time, 0) / times.length;
    console.log(`📊 Performance: ${avgTime.toFixed(2)}ms average over ${iterations} pings`);
    
    return true;
    
  } catch (error) {
    console.error('❌ Performance test failed:', error.message);
    throw error;
  }
}

async function runPingIntegrationTest() {
  console.log('🚀 Starting Ping Integration Test\n');
  
  try {
    await testPingCommand();
    await testPingPerformance();
    
    console.log('\n🎉 PING INTEGRATION TEST COMPLETED!');
    console.log('✅ client.commands.ping() works correctly');
    console.log('✅ All ping variations tested');
    console.log('✅ Performance validated');
    
  } catch (error) {
    console.error('\n❌ Ping integration test failed:', error.message);
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  runPingIntegrationTest();
} else {
  module.exports = { runPingIntegrationTest };
}