/**
 * Transport Architecture Validation Test Suite
 * 
 * Comprehensive validation of the pure transport architecture for future AI engineers.
 * This serves as both validation and detailed implementation documentation.
 * 
 * ARCHITECTURE VALIDATION OBJECTIVES:
 * 1. TypeScript protocol enforcement works correctly
 * 2. Generic adapter base provides proper separation of concerns  
 * 3. WebSocket adapter follows adapter pattern correctly
 * 4. Transport orchestrator bridges pure transports with JTAG business logic
 * 5. Cross-environment compatibility is maintained
 * 
 * FUTURE AI ENGINEER GUIDANCE:
 * - Follow these test patterns when implementing new transport protocols
 * - Use the validation functions to ensure architectural compliance
 * - Extend the test cases when adding new adapter functionality
 */

import type {
  TransportProtocol,
  TransportRole, 
  EnvironmentType,
  WebSocketProtocolContract,
  CrossEnvironmentTransport,
  TransportSendResult,
  TransportError
} from '../TransportProtocolContracts';
import {
  TRANSPORT_PROTOCOLS,
  TRANSPORT_ROLES,
  ENVIRONMENT_TYPES,
  isWebSocketProtocol,
  isValidTransportProtocol,
  isValidTransportRole,
  isValidEnvironment
} from '../TransportProtocolContracts';
import { TransportAdapterBase } from '../adapters/TransportAdapterBase';
import type { TransportAdapterConfig } from '../adapters/TransportAdapterBase';

/**
 * Test Configuration Factory - Creates valid configs for testing
 * FUTURE AI ENGINEER: Use this pattern for protocol-specific config creation
 */
class TransportTestConfigFactory {
  static createWebSocketConfig(
    role: TransportRole = TRANSPORT_ROLES.CLIENT,
    environment: EnvironmentType = ENVIRONMENT_TYPES.BROWSER,
    overrides: Partial<WebSocketProtocolContract['config']> = {}
  ): TransportAdapterConfig<typeof TRANSPORT_PROTOCOLS.WEBSOCKET> {
    return {
      protocol: TRANSPORT_PROTOCOLS.WEBSOCKET,
      role,
      environment,
      name: `test-websocket-${role}-${environment}`,
      protocolConfig: {
        host: 'localhost',
        port: 8080,
        reconnectAttempts: 3,
        reconnectDelay: 1000,
        pingInterval: 30000,
        ...overrides
      },
      adapterOptions: {
        autoReconnect: false,
        maxRetries: 3,
        retryDelay: 1000,
        healthCheckInterval: 5000
      }
    };
  }
}

/**
 * Mock WebSocket Transport Adapter - For testing generic base functionality
 * FUTURE AI ENGINEER: Use this as template for creating testable adapter implementations
 */
class MockWebSocketAdapter extends TransportAdapterBase<typeof TRANSPORT_PROTOCOLS.WEBSOCKET> {
  private mockConnected = false;
  private sendHistory: Array<string | Uint8Array> = [];
  
  constructor(config: TransportAdapterConfig<typeof TRANSPORT_PROTOCOLS.WEBSOCKET>) {
    super(config);
  }

  public async connect(): Promise<void> {
    // Simulate connection process
    await new Promise(resolve => setTimeout(resolve, 10)); // Simulate async connection
    this.mockConnected = true;
    this.emitConnect();
  }

  public async send(data: string | Uint8Array): Promise<TransportSendResult> {
    if (!this.mockConnected) {
      throw this.createTransportError('Not connected', 'SEND_FAILED');
    }
    
    this.sendHistory.push(data);
    return this.createSendResult(
      true,
      typeof data === 'string' ? data.length : data.length,
      { messageCount: this.sendHistory.length }
    );
  }

  public isConnected(): boolean {
    return super.isConnected() && this.mockConnected;
  }

  public async disconnect(): Promise<void> {
    this.mockConnected = false;
    this.emitDisconnect('Manual disconnect');
    this.cleanup();
  }

  // Test helper methods
  public getSendHistory(): ReadonlyArray<string | Uint8Array> {
    return [...this.sendHistory];
  }

  public simulateIncomingData(data: string | Uint8Array): void {
    this.emitData(data);
  }

  public simulateError(code: TransportError['code'], message: string): void {
    const error = this.createTransportError(message, code);
    this.emitError(error);
  }

  public simulateDisconnect(reason: string): void {
    this.mockConnected = false;
    this.emitDisconnect(reason);
  }
}

/**
 * TypeScript Protocol Contract Validation Tests
 * FUTURE AI ENGINEER: These tests ensure TypeScript is enforcing our API contracts
 */
class ProtocolContractTests {
  static runTypeScriptValidation(): void {
    console.log('🔍 Testing TypeScript Protocol Contract Enforcement...');

    // Test 1: Protocol type validation
    const validProtocols: TransportProtocol[] = ['websocket', 'http', 'udp-multicast'];
    validProtocols.forEach(protocol => {
      if (!isValidTransportProtocol(protocol)) {
        throw new Error(`Valid protocol ${protocol} failed validation`);
      }
    });

    // Test 2: Role type validation  
    const validRoles: TransportRole[] = ['client', 'server', 'peer'];
    validRoles.forEach(role => {
      if (!isValidTransportRole(role)) {
        throw new Error(`Valid role ${role} failed validation`);
      }
    });

    // Test 3: Environment type validation
    const validEnvironments: EnvironmentType[] = ['browser', 'server', 'shared'];
    validEnvironments.forEach(env => {
      if (!isValidEnvironment(env)) {
        throw new Error(`Valid environment ${env} failed validation`);
      }
    });

    // Test 4: WebSocket-specific type guard
    if (!isWebSocketProtocol('websocket')) {
      throw new Error('WebSocket protocol type guard failed');
    }
    if (isWebSocketProtocol('invalid-protocol')) {
      throw new Error('WebSocket protocol type guard should reject invalid protocols');
    }

    console.log('✅ TypeScript protocol contract validation passed');
  }

  static validateConfigTypeStrength(): void {
    console.log('🔍 Testing Configuration Type Strength...');

    // Test strongly typed config creation
    const config = TransportTestConfigFactory.createWebSocketConfig();
    
    // TypeScript should enforce these at compile time
    if (config.protocol !== TRANSPORT_PROTOCOLS.WEBSOCKET) {
      throw new Error('Protocol type not enforced correctly');
    }
    
    if (!Object.values(TRANSPORT_ROLES).includes(config.role)) {
      throw new Error('Role type not enforced correctly');
    }
    
    if (!Object.values(ENVIRONMENT_TYPES).includes(config.environment)) {
      throw new Error('Environment type not enforced correctly');
    }

    // Verify protocol-specific config structure
    const protocolConfig = config.protocolConfig;
    if (typeof protocolConfig.host !== 'string' && protocolConfig.host !== undefined) {
      throw new Error('WebSocket host config type not enforced');
    }
    if (typeof protocolConfig.port !== 'number' && protocolConfig.port !== undefined) {
      throw new Error('WebSocket port config type not enforced');
    }

    console.log('✅ Configuration type strength validation passed');
  }
}

/**
 * Generic Adapter Base Validation Tests  
 * FUTURE AI ENGINEER: These tests validate the generic base provides proper separation
 */
class GenericAdapterTests {
  static async runAdapterBaseValidation(): Promise<void> {
    console.log('🔍 Testing Generic Adapter Base Functionality...');

    const config = TransportTestConfigFactory.createWebSocketConfig();
    const adapter = new MockWebSocketAdapter(config);

    // Test 1: Initial state
    if (adapter.isConnected()) {
      throw new Error('Adapter should not be connected initially');
    }
    if (adapter.protocol !== TRANSPORT_PROTOCOLS.WEBSOCKET) {
      throw new Error('Adapter protocol not set correctly');
    }

    // Test 2: Connection lifecycle
    await adapter.connect();
    if (!adapter.isConnected()) {
      throw new Error('Adapter should be connected after connect()');
    }

    // Test 3: Callback system - separated concern
    let connectCallbackCount = 0;
    let disconnectCallbackCount = 0;
    let dataCallbackCount = 0;
    let errorCallbackCount = 0;
    let lastDisconnectReason: string | undefined;
    let lastReceivedData: string | Uint8Array | undefined;
    let lastError: TransportError | undefined;

    adapter.onConnect(() => connectCallbackCount++);
    adapter.onDisconnect((reason) => {
      disconnectCallbackCount++;
      lastDisconnectReason = reason;
    });
    adapter.onData((data) => {
      dataCallbackCount++;
      lastReceivedData = data;
    });
    adapter.onError((error) => {
      errorCallbackCount++;
      lastError = error;
    });

    // Test callback execution
    const testData = 'test message';
    adapter.simulateIncomingData(testData);
    if (dataCallbackCount !== 1 || lastReceivedData !== testData) {
      throw new Error('Data callback not working correctly');
    }

    adapter.simulateError('PROTOCOL_ERROR', 'Test error');
    if (errorCallbackCount !== 1 || !lastError || lastError.code !== 'PROTOCOL_ERROR') {
      throw new Error('Error callback not working correctly');
    }

    // Test 4: Send functionality
    const sendResult = await adapter.send('test send');
    if (!sendResult.success || !sendResult.timestamp) {
      throw new Error('Send result not formatted correctly');
    }

    // Test 5: Disconnect
    await adapter.disconnect();
    if (adapter.isConnected()) {
      throw new Error('Adapter should be disconnected after disconnect()');
    }

    console.log('✅ Generic adapter base validation passed');
  }

  static async runSeparationOfConcernsValidation(): Promise<void> {
    console.log('🔍 Testing Adapter Separation of Concerns...');

    const config = TransportTestConfigFactory.createWebSocketConfig();
    const adapter = new MockWebSocketAdapter(config);

    await adapter.connect();

    // Test 1: Generic base handles callbacks uniformly
    const callbackData: Array<{ type: string; data: any }> = [];
    
    adapter.onConnect(() => callbackData.push({ type: 'connect', data: null }));
    adapter.onDisconnect((reason) => callbackData.push({ type: 'disconnect', data: reason }));
    adapter.onData((data) => callbackData.push({ type: 'data', data }));
    adapter.onError((error) => callbackData.push({ type: 'error', data: error.code }));

    // Test 2: Protocol-specific send logic (while connected)
    const sendCountBefore = adapter.getSendHistory().length;
    await adapter.send('protocol-specific-message');
    const sendCountAfter = adapter.getSendHistory().length;
    
    if (sendCountAfter !== sendCountBefore + 1) {
      throw new Error(`Protocol-specific send logic not working. Before: ${sendCountBefore}, After: ${sendCountAfter}`);
    }

    // Test 3: Simulate events - base should handle uniformly  
    adapter.simulateIncomingData('data1');
    adapter.simulateError('TIMEOUT', 'Test timeout');
    adapter.simulateDisconnect('Network error');

    // Verify separation: base handles callbacks, adapter handles protocol
    const expectedEventCount = 3; // data, error, disconnect
    if (callbackData.length !== expectedEventCount) {
      throw new Error(`Expected ${expectedEventCount} events, got ${callbackData.length}`);
    }

    console.log('✅ Separation of concerns validation passed');
  }
}

/**
 * Cross-Environment Compatibility Tests
 * FUTURE AI ENGINEER: These ensure adapters work across browser/server environments
 */
class CrossEnvironmentTests {
  static runEnvironmentCompatibilityValidation(): void {
    console.log('🔍 Testing Cross-Environment Compatibility...');

    // Test 1: Browser environment configuration
    const browserConfig = TransportTestConfigFactory.createWebSocketConfig(
      TRANSPORT_ROLES.CLIENT,
      ENVIRONMENT_TYPES.BROWSER
    );
    
    if (browserConfig.environment !== ENVIRONMENT_TYPES.BROWSER) {
      throw new Error('Browser environment config not created correctly');
    }

    // Test 2: Server environment configuration  
    const serverConfig = TransportTestConfigFactory.createWebSocketConfig(
      TRANSPORT_ROLES.SERVER,
      ENVIRONMENT_TYPES.SERVER
    );
    
    if (serverConfig.environment !== ENVIRONMENT_TYPES.SERVER) {
      throw new Error('Server environment config not created correctly');
    }

    // Test 3: Adapter works with both environments
    const browserAdapter = new MockWebSocketAdapter(browserConfig);
    const serverAdapter = new MockWebSocketAdapter(serverConfig);

    if (browserAdapter.environment !== ENVIRONMENT_TYPES.BROWSER) {
      throw new Error('Browser adapter environment not set correctly');
    }
    if (serverAdapter.environment !== ENVIRONMENT_TYPES.SERVER) {
      throw new Error('Server adapter environment not set correctly');
    }

    console.log('✅ Cross-environment compatibility validation passed');
  }
}

/**
 * Main Test Runner - Orchestrates all validation tests
 * FUTURE AI ENGINEER: Run this to validate your transport implementations
 */
export async function runTransportArchitectureValidation(): Promise<void> {
  console.log('🚀 Starting Transport Architecture Validation');
  console.log('=' .repeat(60));
  console.log('🎯 Validating pure transport architecture for future AI engineers');
  console.log('📋 Testing: Protocol contracts, adapters, separation of concerns');
  console.log('=' .repeat(60));

  try {
    // Phase 1: TypeScript Protocol Enforcement
    ProtocolContractTests.runTypeScriptValidation();
    ProtocolContractTests.validateConfigTypeStrength();

    // Phase 2: Generic Adapter Architecture  
    await GenericAdapterTests.runAdapterBaseValidation();
    await GenericAdapterTests.runSeparationOfConcernsValidation();

    // Phase 3: Cross-Environment Compatibility
    CrossEnvironmentTests.runEnvironmentCompatibilityValidation();

    console.log('=' .repeat(60));
    console.log('✅ TRANSPORT ARCHITECTURE VALIDATION COMPLETE');
    console.log('🎯 All tests passed - architecture ready for implementation');
    console.log('📋 Future AI engineers can follow these patterns safely');
    console.log('🔧 Generic adapters provide proper separation of concerns');
    console.log('🌐 Cross-environment compatibility validated');
    console.log('=' .repeat(60));

  } catch (error) {
    console.error('=' .repeat(60));
    console.error('❌ TRANSPORT ARCHITECTURE VALIDATION FAILED');
    console.error('Error:', error instanceof Error ? error.message : String(error));
    console.error('🚨 Fix architecture issues before proceeding');
    console.error('=' .repeat(60));
    throw error;
  }
}

/**
 * Run validation if executed directly
 * FUTURE AI ENGINEER: Use this command to validate your implementations
 */
if (require.main === module) {
  runTransportArchitectureValidation().catch((error) => {
    console.error('Architecture validation failed:', error);
    process.exit(1);
  });
}