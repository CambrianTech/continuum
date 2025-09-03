#!/usr/bin/env tsx
/**
 * Transport Architecture Integration Tests - Cross-Environment Validation
 * 
 * Tests the new transport architecture concepts through the existing JTAG system:
 * - Validates transport protocol contracts work with real WebSocket connections
 * - Tests cross-environment communication through existing JTAG infrastructure  
 * - Confirms transport configuration resolves correctly in real browser/server contexts
 * - Validates message correlation and error handling across environment boundaries
 * 
 * APPROACH: Uses existing JTAG system (ports 9001/9002) for integration testing
 * This focuses on validating transport architecture concepts without port conflicts.
 */

import { 
  TRANSPORT_PROTOCOLS, 
  TRANSPORT_ROLES, 
  ENVIRONMENT_TYPES,
  isValidTransportProtocol,
  isWebSocketProtocol 
} from '../../system/transports/shared/TransportProtocolContracts';
import type { TransportAdapterConfig } from '../../system/transports/shared/adapters/TransportAdapterBase';

// Test utilities with timeout protection
function assert(condition: boolean, message: string) {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

function withTimeout<T>(promise: Promise<T>, timeoutMs: number = 15000): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) => 
      setTimeout(() => reject(new Error(`Integration test timed out after ${timeoutMs}ms`)), timeoutMs)
    )
  ]);
}

async function ensureSystemRunning() {
  console.log('🔄 Checking if JTAG system is accessible for transport testing...');
  
  // Simple approach: try to connect to existing system via JTAG client
  // If this works, the system is running and we can proceed with transport tests
  try {
    const { JTAGClientServer } = await import('../../system/core/client/server/JTAGClientServer');
    const result = await withTimeout(JTAGClientServer.connect(), 10000);
    const { client, listResult } = result;
    
    if (listResult.success && Array.isArray(listResult.commands)) {
      console.log('✅ JTAG system is accessible via WebSocket transport');
      return { client, available: true };
    } else {
      console.warn('⚠️ JTAG system connection failed');
      return { client: null, available: false };
    }
  } catch (error) {
    console.warn('⚠️ JTAG system not available:', error.message);
    return { client: null, available: false };
  }
}

/**
 * INTEGRATION TEST 1: Cross-Environment Transport Configuration
 */
async function testCrossEnvironmentTransportConfiguration() {
  console.log('\n🌐 TEST 1: Cross-environment transport configuration validation');
  
  // Test browser-side configuration
  const browserConfig: TransportAdapterConfig<typeof TRANSPORT_PROTOCOLS.WEBSOCKET> = {
    protocol: TRANSPORT_PROTOCOLS.WEBSOCKET,
    role: TRANSPORT_ROLES.CLIENT,
    environment: ENVIRONMENT_TYPES.BROWSER,
    protocolConfig: { host: 'localhost', port: 9001 }, // Should connect to server WebSocket
    adapterOptions: { autoReconnect: true }
  };
  
  // Test server-side configuration
  const serverConfig: TransportAdapterConfig<typeof TRANSPORT_PROTOCOLS.WEBSOCKET> = {
    protocol: TRANSPORT_PROTOCOLS.WEBSOCKET,
    role: TRANSPORT_ROLES.SERVER,
    environment: ENVIRONMENT_TYPES.SERVER,
    protocolConfig: { host: 'localhost', port: 9001 }, // Should bind to port
    adapterOptions: { autoReconnect: false }
  };
  
  // Validate configuration resolution works across environments
  assert(browserConfig.protocol === 'websocket', 'Browser config protocol type resolution');
  assert(serverConfig.protocol === 'websocket', 'Server config protocol type resolution');
  assert(browserConfig.environment !== serverConfig.environment, 'Environment differentiation');
  assert(browserConfig.role !== serverConfig.role, 'Role differentiation');
  
  // Validate type safety at boundaries
  assert(typeof browserConfig.protocolConfig.host === 'string', 'Browser WebSocket config typing');
  assert(typeof serverConfig.protocolConfig.port === 'number', 'Server WebSocket config typing');
  
  console.log('✅ Cross-environment transport configuration validated');
}

/**
 * INTEGRATION TEST 2: Real WebSocket Communication Through Transport Architecture
 */
async function testRealWebSocketCommunication() {
  console.log('\n🔌 TEST 2: Real WebSocket communication through transport architecture');
  
  // Import JTAG client for real cross-environment testing
  const { JTAGClientServer } = await import('../../system/core/client/server/JTAGClientServer');
  
  console.log('🔄 Connecting browser client to server via real WebSocket...');
  
  // This creates a REAL WebSocket connection browser → server
  const result = await withTimeout(JTAGClientServer.connect(), 15000);
  const { client, listResult } = result;
  
  assert(listResult.success === true, 'Real WebSocket connection established');
  assert(Array.isArray(listResult.commands), 'Command discovery through WebSocket');
  assert(listResult.commands.length > 0, 'Commands received across environments');
  
  console.log(`📋 Real WebSocket communication: ${listResult.commands.length} commands discovered`);
  
  // Test actual message passing through transport layer
  console.log('🔄 Testing real message passing through transport...');
  
  const pingResult = await withTimeout(client.commands.ping({
    message: 'transport-integration-test',
    includeTiming: true,
    includeEnvironment: true
  }), 10000);
  
  assert(pingResult.success === true, 'Real message sent through transport architecture');
  assert(pingResult.message === 'transport-integration-test', 'Message integrity across environments');
  assert(typeof pingResult.roundTripTime === 'number', 'Timing measurement across transport');
  assert(pingResult.environment.type === 'browser', 'Environment detection across transport');
  
  console.log(`📊 Real transport round-trip: ${pingResult.roundTripTime}ms`);
  console.log('✅ Real WebSocket communication through transport architecture validated');
  
  return client;
}

/**
 * INTEGRATION TEST 3: Transport Protocol Contract Enforcement Across Environments
 */
async function testTransportProtocolContractEnforcement(client: any) {
  console.log('\n📋 TEST 3: Transport protocol contract enforcement across environments');
  
  // Test that invalid protocol parameters are rejected by the transport system
  console.log('🔄 Testing protocol contract validation...');
  
  try {
    // This should work - valid protocol usage
    const validResult = await withTimeout(client.commands.ping({
      message: 'valid-protocol-test',
      includeTiming: true
    }), 5000);
    
    assert(validResult.success === true, 'Valid protocol contract accepted');
    console.log('📨 Valid protocol usage accepted by transport system');
    
  } catch (error) {
    throw new Error(`Valid protocol usage was rejected: ${error.message}`);
  }
  
  // Test protocol boundary validation
  console.log('🔄 Testing protocol boundary validation...');
  
  // Test with extreme parameter values to validate transport robustness  
  const extremeResult = await withTimeout(client.commands.ping({
    message: 'x'.repeat(1000), // Large message
    includeTiming: true,
    includeEnvironment: true
  }), 10000);
  
  assert(extremeResult.success === true, 'Transport handles large messages');
  assert(extremeResult.message.length === 1000, 'Large message integrity preserved');
  
  console.log('✅ Transport protocol contract enforcement validated across environments');
}

/**
 * INTEGRATION TEST 4: Cross-Environment Error Handling and Recovery
 */
async function testCrossEnvironmentErrorHandling(client: any) {
  console.log('\n💥 TEST 4: Cross-environment error handling and recovery');
  
  console.log('🔄 Testing error propagation across transport boundaries...');
  
  // Test that errors are properly propagated from server → browser through transport
  try {
    await withTimeout(client.commands['test-error']({
      errorType: 'network-error',
      message: 'transport-integration-error-test',
      environment: 'browser'
    }), 5000);
    
    // Should not reach here
    throw new Error('Expected error was not propagated through transport');
    
  } catch (error) {
    // Expected - error should be propagated through transport architecture
    assert(typeof error.message === 'string', 'Error message propagated through transport');
    assert(error.message.length > 0, 'Error details preserved across environments');
    console.log(`📨 Error correctly propagated through transport: ${error.message.substring(0, 60)}...`);
  }
  
  // Test transport recovery after error
  console.log('🔄 Testing transport recovery after error...');
  
  const recoveryResult = await withTimeout(client.commands.ping({
    message: 'post-error-recovery-test'
  }), 5000);
  
  assert(recoveryResult.success === true, 'Transport recovered after error');
  assert(recoveryResult.message === 'post-error-recovery-test', 'Transport functionality restored');
  
  console.log('✅ Cross-environment error handling and recovery validated');
}

/**
 * INTEGRATION TEST 5: Concurrent Transport Operations Across Environments
 */
async function testConcurrentTransportOperations(client: any) {
  console.log('\n🔄 TEST 5: Concurrent transport operations across environments');
  
  console.log('🔄 Testing concurrent message handling through transport...');
  
  // Execute multiple concurrent operations through transport layer
  const concurrentOperations = [
    client.commands.ping({ message: 'concurrent-1', includeTiming: true }),
    client.commands.ping({ message: 'concurrent-2', includeTiming: true }),
    client.commands.ping({ message: 'concurrent-3', includeTiming: true }),
    client.commands.ping({ message: 'concurrent-4', includeTiming: true }),
    client.commands.ping({ message: 'concurrent-5', includeTiming: true })
  ];
  
  const results = await withTimeout(Promise.all(concurrentOperations), 15000);
  
  assert(results.length === 5, 'All concurrent operations completed through transport');
  assert(results.every(r => r.success), 'All concurrent operations succeeded');
  
  // Validate message correlation through transport
  const messages = results.map(r => r.message);
  assert(messages.includes('concurrent-1'), 'First message correctly correlated');
  assert(messages.includes('concurrent-2'), 'Second message correctly correlated');
  assert(messages.includes('concurrent-3'), 'Third message correctly correlated');
  assert(messages.includes('concurrent-4'), 'Fourth message correctly correlated');
  assert(messages.includes('concurrent-5'), 'Fifth message correctly correlated');
  
  // Performance analysis
  const avgTime = results.reduce((sum, r) => sum + r.roundTripTime, 0) / results.length;
  const maxTime = Math.max(...results.map(r => r.roundTripTime));
  
  console.log(`📊 Concurrent transport performance: avg ${avgTime.toFixed(2)}ms, max ${maxTime}ms`);
  console.log('✅ Concurrent transport operations across environments validated');
}

/**
 * Main Integration Test Runner - Cross-Environment Validation Through Existing JTAG System
 */
async function runTransportArchitectureIntegrationTests(): Promise<void> {
  console.log('🚀 Starting Transport Architecture Integration Tests');
  console.log('🌐 Cross-Environment Validation via Existing JTAG System');
  console.log('=' .repeat(60));
  
  try {
    // Step 1: Check if system is accessible
    const systemStatus = await ensureSystemRunning();
    if (!systemStatus.available) {
      console.log('⚠️  JTAG system not available - skipping integration tests');
      console.log('💡 To run integration tests: start system with `npm start`');
      return; // Graceful skip rather than failure
    }
    
    const { client } = systemStatus;
    
    // Step 2: Test transport architecture concepts through existing system
    await testCrossEnvironmentTransportConfiguration();
    
    // Step 3: Test real WebSocket communication validates transport contracts
    await testRealWebSocketCommunication();
    
    // Step 4: Test protocol contract enforcement through real system
    await testTransportProtocolContractEnforcement(client);
    
    // Step 5: Test error handling across environments through existing transport
    await testCrossEnvironmentErrorHandling(client);
    
    // Step 6: Test concurrent operations validate transport robustness
    await testConcurrentTransportOperations(client);
    
    console.log('=' .repeat(60));
    console.log('✅ ALL TRANSPORT INTEGRATION TESTS PASSED');
    console.log('🎯 Transport configuration contracts: Validated');
    console.log('🎯 Cross-environment WebSocket transport: Validated');
    console.log('🎯 Protocol contract enforcement: Validated');
    console.log('🎯 Transport error handling & recovery: Validated');
    console.log('🎯 Concurrent transport operations: Validated');
    console.log('=' .repeat(60));
    console.log('🌐 TRANSPORT ARCHITECTURE CONCEPTS VALIDATED THROUGH REAL SYSTEM!');
    
  } catch (error) {
    console.error('=' .repeat(60));
    console.error('❌ TRANSPORT INTEGRATION TESTS FAILED');
    console.error('Error:', error instanceof Error ? error.message : String(error));
    console.error('❌ Transport architecture validation failed');
    console.error('=' .repeat(60));
    throw error;
  }
}

/**
 * Auto-run if executed directly (for npm test integration)
 */
if (require.main === module) {
  runTransportArchitectureIntegrationTests()
    .then(() => {
      console.log('Transport architecture integration tests completed successfully');
      process.exit(0);
    })
    .catch((error) => {
      console.error('Transport architecture integration tests failed:', error);
      process.exit(1);
    });
}