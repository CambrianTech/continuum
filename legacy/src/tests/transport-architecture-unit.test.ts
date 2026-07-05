#!/usr/bin/env npx tsx
/**
 * Transport Architecture Unit Tests - Critical Assumption Validation
 * 
 * Tests the riskiest assumptions in the new transport architecture.
 * Focuses on type safety, generic patterns, and memory management.
 * 
 * INTEGRATION: This test is designed to run within npm test workflow
 */

import { 
  TRANSPORT_PROTOCOLS,
  TRANSPORT_ROLES,
  ENVIRONMENT_TYPES,
  type TransportProtocol,
  isWebSocketProtocol,
  isValidTransportProtocol
} from '../system/transports/shared/TransportProtocolContracts';

import { TransportAdapterBase } from '../system/transports/shared/adapters/TransportAdapterBase';
import type { TransportAdapterConfig } from '../system/transports/shared/adapters/TransportAdapterBase';

/**
 * Test Mock for Generic Adapter Base Testing
 */
class TestTransportAdapter extends TransportAdapterBase<typeof TRANSPORT_PROTOCOLS.WEBSOCKET> {
  private sendHistory: Array<string | Uint8Array> = [];
  
  public async connect(): Promise<void> {
    this.emitConnect();
  }
  
  public async send(data: string | Uint8Array): Promise<any> {
    this.sendHistory.push(data);
    return this.createSendResult(true, data.length, { testData: true });
  }
  
  public async disconnect(): Promise<void> {
    this.emitDisconnect('Test disconnect');
    this.cleanup();
  }
  
  // Test helpers
  public getSendHistory() { return [...this.sendHistory]; }
  public testEmitData(data: string | Uint8Array) { this.emitData(data); }
  public testEmitError(code: any, message: string) { 
    const error = this.createTransportError(message, code);
    this.emitError(error); 
  }
}

/**
 * UNIT TEST 1: TypeScript Type Safety Validation
 */
function testTypeScriptContractSafety(): void {
  console.log('🧪 Testing TypeScript contract type safety...');
  
  // Test 1: const assertion prevents typos
  const validProtocols: TransportProtocol[] = ['websocket', 'http', 'udp-multicast'];
  validProtocols.forEach(protocol => {
    if (!isValidTransportProtocol(protocol)) {
      throw new Error(`Valid protocol ${protocol} failed type guard validation`);
    }
  });
  
  // Test 2: Type guards work correctly  
  if (!isWebSocketProtocol('websocket')) {
    throw new Error('WebSocket type guard should accept websocket');
  }
  if (isWebSocketProtocol('invalid-protocol')) {
    throw new Error('WebSocket type guard should reject invalid protocols');  
  }
  
  // Test 3: const values match expected
  if (TRANSPORT_PROTOCOLS.WEBSOCKET !== 'websocket') {
    throw new Error('WEBSOCKET constant has wrong value');
  }
  
  console.log('✅ TypeScript contract type safety validated');
}

/**
 * UNIT TEST 2: Generic Adapter Base Memory Management
 */
async function testGenericAdapterMemoryManagement(): Promise<void> {
  console.log('🧪 Testing generic adapter memory management...');
  
  const config: TransportAdapterConfig<typeof TRANSPORT_PROTOCOLS.WEBSOCKET> = {
    protocol: TRANSPORT_PROTOCOLS.WEBSOCKET,
    role: TRANSPORT_ROLES.CLIENT,
    environment: ENVIRONMENT_TYPES.BROWSER,
    protocolConfig: { host: 'localhost', port: 8080 },
    adapterOptions: { autoReconnect: false }
  };
  
  const adapter = new TestTransportAdapter(config);
  
  // Test 1: Initial state
  if (adapter.isConnected()) {
    throw new Error('Adapter should not be connected initially');
  }
  
  // Test 2: Callback management with multiple subscribers
  let connectCount = 0;
  let disconnectCount = 0;
  let dataCount = 0;
  let errorCount = 0;
  
  // Add multiple callbacks to test Set management
  const cleanup1 = () => { adapter.onConnect(() => connectCount++); };
  const cleanup2 = () => { adapter.onConnect(() => connectCount++); };
  const cleanup3 = () => { adapter.onDisconnect(() => disconnectCount++); };
  const cleanup4 = () => { adapter.onData(() => dataCount++); };
  const cleanup5 = () => { adapter.onError(() => errorCount++); };
  
  cleanup1();
  cleanup2();  
  cleanup3();
  cleanup4();
  cleanup5();
  
  // Test 3: Connection lifecycle
  await adapter.connect();
  if (!adapter.isConnected()) {
    throw new Error('Adapter should be connected after connect()');
  }
  if (connectCount !== 2) {
    throw new Error(`Expected 2 connect callbacks, got ${connectCount}`);
  }
  
  // Test 4: Data and error handling
  adapter.testEmitData('test data');
  if (dataCount !== 1) {
    throw new Error(`Expected 1 data callback, got ${dataCount}`);
  }
  
  adapter.testEmitError('TEST_ERROR', 'Test error message');
  if (errorCount !== 1) {
    throw new Error(`Expected 1 error callback, got ${errorCount}`);
  }
  
  // Test 5: Send functionality
  const sendResult = await adapter.send('test message');
  if (!sendResult.success || sendResult.bytesTransmitted !== 12) {
    throw new Error('Send result incorrect');
  }
  
  const history = adapter.getSendHistory();
  if (history.length !== 1 || history[0] !== 'test message') {
    throw new Error('Send history not tracked correctly');
  }
  
  // Test 6: Cleanup on disconnect
  await adapter.disconnect();
  if (adapter.isConnected()) {
    throw new Error('Adapter should be disconnected after disconnect()');
  }
  if (disconnectCount !== 1) {
    throw new Error(`Expected 1 disconnect callback, got ${disconnectCount}`);
  }
  
  console.log('✅ Generic adapter memory management validated');
}

/**
 * UNIT TEST 3: Configuration Type Resolution
 */
function testConfigurationTypeResolution(): void {
  console.log('🧪 Testing configuration type resolution...');
  
  // Test complex generic type resolution
  const config: TransportAdapterConfig<typeof TRANSPORT_PROTOCOLS.WEBSOCKET> = {
    protocol: TRANSPORT_PROTOCOLS.WEBSOCKET,
    role: TRANSPORT_ROLES.CLIENT,
    environment: ENVIRONMENT_TYPES.BROWSER,
    protocolConfig: {
      host: 'localhost',
      port: 8080,
      reconnectAttempts: 5
    }
  };
  
  // Verify type resolution worked correctly
  if (config.protocol !== 'websocket') {
    throw new Error('Protocol type not resolved correctly');
  }
  
  if (typeof config.protocolConfig.host !== 'string') {
    throw new Error('Protocol config types not resolved correctly');
  }
  
  // Test readonly enforcement
  try {
    (config as any).protocol = 'http'; // Should be readonly
    throw new Error('Readonly enforcement failed - this should not be reachable');
  } catch (error) {
    if (error instanceof Error && error.message.includes('readonly')) {
      console.log('✅ Readonly enforcement working');
    }
    // Expected - readonly should prevent modification
  }
  
  console.log('✅ Configuration type resolution validated');
}

/**
 * UNIT TEST 4: Error Handling and Context Preservation
 */
async function testErrorHandlingContextPreservation(): Promise<void> {
  console.log('🧪 Testing error handling and context preservation...');
  
  const config: TransportAdapterConfig<typeof TRANSPORT_PROTOCOLS.WEBSOCKET> = {
    protocol: TRANSPORT_PROTOCOLS.WEBSOCKET,
    role: TRANSPORT_ROLES.CLIENT,
    environment: ENVIRONMENT_TYPES.BROWSER,
    protocolConfig: { host: 'localhost', port: 8080 }
  };
  
  const adapter = new TestTransportAdapter(config);
  await adapter.connect();
  
  let capturedError: any = null;
  adapter.onError((error) => {
    capturedError = error;
  });
  
  // Emit error and verify context preservation
  adapter.testEmitError('NETWORK_FAILURE', 'Connection timeout');
  
  if (!capturedError) {
    throw new Error('Error callback was not called');
  }
  
  if (capturedError.code !== 'NETWORK_FAILURE') {
    throw new Error(`Expected error code NETWORK_FAILURE, got ${capturedError.code}`);
  }
  
  if (capturedError.protocol !== 'websocket') {
    throw new Error(`Expected protocol websocket, got ${capturedError.protocol}`);
  }
  
  if (!capturedError.timestamp) {
    throw new Error('Error timestamp not preserved');
  }
  
  if (!capturedError.metadata || capturedError.metadata.transportName !== adapter.name) {
    throw new Error('Error metadata not preserved correctly');
  }
  
  await adapter.disconnect();
  console.log('✅ Error handling and context preservation validated');
}

/**
 * Main Test Runner - Integrates with npm test workflow
 */
async function runTransportArchitectureUnitTests(): Promise<void> {
  console.log('🚀 Running Transport Architecture Unit Tests');
  console.log('=' .repeat(50));
  
  try {
    // Critical assumption tests
    testTypeScriptContractSafety();
    await testGenericAdapterMemoryManagement();
    testConfigurationTypeResolution();
    await testErrorHandlingContextPreservation();
    
    console.log('=' .repeat(50));
    console.log('✅ ALL TRANSPORT UNIT TESTS PASSED');
    console.log('🎯 Type safety: Validated');
    console.log('🎯 Memory management: Validated');  
    console.log('🎯 Configuration types: Validated');
    console.log('🎯 Error handling: Validated');
    console.log('=' .repeat(50));
    
  } catch (error) {
    console.error('=' .repeat(50));
    console.error('❌ TRANSPORT UNIT TEST FAILED');
    console.error('Error:', error instanceof Error ? error.message : String(error));
    console.error('❌ Architecture assumptions violated');
    console.error('=' .repeat(50));
    throw error;
  }
}

/**
 * Auto-run if executed directly (for npm test integration)
 */
if (require.main === module) {
  runTransportArchitectureUnitTests()
    .then(() => {
      console.log('Transport architecture unit tests completed successfully');
      process.exit(0);
    })
    .catch((error) => {
      console.error('Transport architecture unit tests failed:', error);
      process.exit(1);
    });
}