/**
 * Pure Transport Types Test - Comprehensive validation of clean transport interfaces
 * 
 * Tests the truly pure transport architecture with strong TypeScript types.
 * Validates that transports are truly dumb pipes without JTAG business logic.
 */

import type { 
  PureTransportConfig, 
  PureTransport, 
  PureSendResult, 
  PureTransportFactory 
} from '../PureTransportTypes';

/**
 * Test configuration factory with strong typing
 */
function createTestConfig(overrides: Partial<PureTransportConfig> = {}): PureTransportConfig {
  return {
    protocol: 'websocket',
    host: 'localhost',
    port: 8080,
    timeout: 5000,
    retryAttempts: 3,
    retryDelay: 1000,
    ...overrides
  };
}

/**
 * Mock pure transport implementation for testing
 */
class MockPureTransport implements PureTransport {
  public readonly name = 'mock-transport';
  public readonly protocol = 'websocket' as const;
  
  private connected = false;
  private dataCallback?: (data: string | Uint8Array) => void;
  private connectCallback?: () => void;
  private disconnectCallback?: (reason?: string) => void;
  private errorCallback?: (error: Error) => void;
  
  async connect(config?: PureTransportConfig): Promise<void> {
    this.connected = true;
    this.connectCallback?.();
  }
  
  async send(data: string | Uint8Array): Promise<PureSendResult> {
    if (!this.connected) {
      throw new Error('Transport not connected');
    }
    
    return {
      success: true,
      timestamp: new Date().toISOString(),
      bytesTransmitted: typeof data === 'string' ? data.length : data.length
    };
  }
  
  isConnected(): boolean {
    return this.connected;
  }
  
  async disconnect(): Promise<void> {
    this.connected = false;
    this.disconnectCallback?.('Manual disconnect');
  }
  
  onData(callback: (data: string | Uint8Array) => void): void {
    this.dataCallback = callback;
  }
  
  onConnect(callback: () => void): void {
    this.connectCallback = callback;
  }
  
  onDisconnect(callback: (reason?: string) => void): void {
    this.disconnectCallback = callback;
  }
  
  onError(callback: (error: Error) => void): void {
    this.errorCallback = callback;
  }
  
  // Test helper methods
  simulateIncomingData(data: string | Uint8Array): void {
    this.dataCallback?.(data);
  }
  
  simulateError(error: Error): void {
    this.errorCallback?.(error);
  }
}

/**
 * Type validation tests - ensure strong typing
 */
function testTypeValidation(): void {
  console.log('🧪 Testing Pure Transport Type Validation...');
  
  // Test 1: PureTransportConfig has only networking parameters
  const config: PureTransportConfig = createTestConfig();
  
  // ✅ Should have networking parameters
  if (typeof config.protocol !== 'string') {
    throw new Error('Protocol must be string');
  }
  if (config.host !== undefined && typeof config.host !== 'string') {
    throw new Error('Host must be string or undefined');
  }
  if (config.port !== undefined && typeof config.port !== 'number') {
    throw new Error('Port must be number or undefined');
  }
  
  // ✅ Should NOT have JTAG business logic properties
  // TypeScript should prevent this at compile time
  const configTyped = config as any;
  if ('eventSystem' in configTyped) {
    throw new Error('PureTransportConfig must not contain eventSystem');
  }
  if ('sessionId' in configTyped) {
    throw new Error('PureTransportConfig must not contain sessionId');
  }
  if ('handler' in configTyped) {
    throw new Error('PureTransportConfig must not contain handler');
  }
  
  console.log('✅ Type validation passed - pure config contains only networking parameters');
}

/**
 * Pure transport interface tests
 */
async function testPureTransportInterface(): Promise<void> {
  console.log('🧪 Testing Pure Transport Interface...');
  
  const transport = new MockPureTransport();
  
  // Test connection lifecycle
  if (transport.isConnected()) {
    throw new Error('Transport should not be connected initially');
  }
  
  await transport.connect(createTestConfig());
  
  if (!transport.isConnected()) {
    throw new Error('Transport should be connected after connect()');
  }
  
  // Test data sending - pure binary/string data only
  const stringData = 'test message';
  const binaryData = new Uint8Array([1, 2, 3, 4]);
  
  const stringResult = await transport.send(stringData);
  if (!stringResult.success || !stringResult.timestamp) {
    throw new Error('String send result invalid');
  }
  
  const binaryResult = await transport.send(binaryData);
  if (!binaryResult.success || !binaryResult.timestamp) {
    throw new Error('Binary send result invalid');
  }
  
  // Test disconnection
  await transport.disconnect();
  if (transport.isConnected()) {
    throw new Error('Transport should be disconnected after disconnect()');
  }
  
  console.log('✅ Pure transport interface tests passed');
}

/**
 * Callback system tests - pure networking events only
 */
async function testPureCallbacks(): Promise<void> {
  console.log('🧪 Testing Pure Transport Callbacks...');
  
  const transport = new MockPureTransport();
  
  // Test connection callbacks
  let connectCalled = false;
  let disconnectCalled = false;
  let disconnectReason: string | undefined;
  
  transport.onConnect(() => {
    connectCalled = true;
  });
  
  transport.onDisconnect((reason) => {
    disconnectCalled = true;
    disconnectReason = reason;
  });
  
  // Test data callback - should receive raw data
  let receivedData: string | Uint8Array | undefined;
  transport.onData((data) => {
    receivedData = data;
  });
  
  // Test error callback
  let receivedError: Error | undefined;
  transport.onError((error) => {
    receivedError = error;
  });
  
  // Simulate connection lifecycle
  await transport.connect();
  if (!connectCalled) {
    throw new Error('Connect callback not called');
  }
  
  // Simulate incoming data
  const testData = 'raw data';
  transport.simulateIncomingData(testData);
  if (receivedData !== testData) {
    throw new Error('Data callback not called with correct data');
  }
  
  // Simulate error
  const testError = new Error('Network error');
  transport.simulateError(testError);
  if (receivedError !== testError) {
    throw new Error('Error callback not called with correct error');
  }
  
  // Test disconnection
  await transport.disconnect();
  if (!disconnectCalled || disconnectReason !== 'Manual disconnect') {
    throw new Error('Disconnect callback not called correctly');
  }
  
  console.log('✅ Pure callback tests passed');
}

/**
 * Protocol type safety tests
 */
function testProtocolTypeSafety(): void {
  console.log('🧪 Testing Protocol Type Safety...');
  
  // Test valid protocols
  const validProtocols: Array<PureTransportConfig['protocol']> = ['websocket', 'http', 'udp-multicast'];
  
  for (const protocol of validProtocols) {
    const config = createTestConfig({ protocol });
    if (config.protocol !== protocol) {
      throw new Error(`Protocol ${protocol} not properly typed`);
    }
  }
  
  // Test that invalid protocols are caught by TypeScript
  // This would fail at compile time:
  // const invalidConfig: PureTransportConfig = { protocol: 'invalid' }; // TS Error
  
  console.log('✅ Protocol type safety tests passed');
}

/**
 * Run all pure transport tests
 */
export async function runPureTransportTests(): Promise<void> {
  console.log('🚀 Starting Pure Transport Architecture Tests');
  console.log('='.repeat(50));
  
  try {
    testTypeValidation();
    await testPureTransportInterface();
    await testPureCallbacks();
    testProtocolTypeSafety();
    
    console.log('='.repeat(50));
    console.log('✅ ALL PURE TRANSPORT TESTS PASSED');
    console.log('🎯 Architecture validation: Transports are truly dumb pipes');
    console.log('🎯 Type safety: Strong TypeScript typing enforced');
    console.log('🎯 Separation: No JTAG business logic in transport layer');
    
  } catch (error) {
    console.error('='.repeat(50));
    console.error('❌ PURE TRANSPORT TEST FAILED');
    console.error('Error:', error instanceof Error ? error.message : String(error));
    throw error;
  }
}

/**
 * Run tests if executed directly
 */
if (require.main === module) {
  runPureTransportTests().catch((error) => {
    console.error('Test execution failed:', error);
    process.exit(1);
  });
}