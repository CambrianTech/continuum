#!/usr/bin/env tsx
/**
 * Client Connection Logic Unit Tests
 * 
 * Tests the JTAG client connection system including local/remote connection logic,
 * command discovery, session management, and connection abstraction patterns.
 */

import { JTAGClient, LocalConnection, RemoteConnection, type JTAGConnection, type JTAGClientConnectOptions } from '../../system/core/client/shared/JTAGClient';
import { JTAGClientBrowser } from '../../system/core/client/browser/JTAGClientBrowser';
import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';
import type { JTAGContext, JTAGMessage, JTAGPayload } from '../../system/core/types/JTAGTypes';
import type { ListResult, CommandSignature } from '../../commands/list/shared/ListTypes';
import type { JTAGSystem } from '../../system/core/system/shared/JTAGSystem';
import type { CommandsInterface } from '../../system/core/shared/JTAGBase';
import { SYSTEM_SCOPES } from '../../system/core/types/SystemScopes';

console.log('🧪 Client Connection Logic Unit Test Suite');

// Mock JTAGSystem for testing
class MockJTAGSystem {
  public readonly name = 'MockJTAGSystem';
  public readonly context: JTAGContext;
  public systemDaemons: any[] = [];

  constructor(context: JTAGContext) {
    this.context = context;
    
    // Mock SessionDaemon for session management tests
    this.systemDaemons.push({
      name: 'session-daemon',
      async getOrCreateSession(): Promise<string> {
        return 'real-session-' + Date.now();
      }
    });
  }

  static async connect(): Promise<MockJTAGSystem> {
    const context: JTAGContext = { uuid: 'mock-system', environment: 'server' };
    return new MockJTAGSystem(context);
  }

  getCommandsInterface(): CommandsInterface {
    const commands = new Map();
    commands.set('list', { name: 'list', description: 'List commands' });
    commands.set('ping', { name: 'ping', description: 'Ping command' });
    commands.set('screenshot', { name: 'screenshot', description: 'Take screenshot' });
    return commands;
  }

  get commands() {
    return {
      list: async (params: any) => ({
        success: true,
        commands: [
          { name: 'list', description: 'List available commands', signature: {} },
          { name: 'ping', description: 'Test connectivity', signature: {} },
          { name: 'screenshot', description: 'Take screenshot', signature: {} }
        ],
        totalCount: 3,
        category: 'all'
      }),
      ping: async (params: any) => ({ success: true, message: 'pong' }),
      screenshot: async (params: any) => ({ success: true, filename: 'test.png' })
    };
  }
}

// Test client implementation
class TestJTAGClient extends JTAGClient {
  private mockLocalSystem?: MockJTAGSystem;
  private shouldReturnLocalSystem = true;

  constructor(context: JTAGContext, returnLocalSystem = true) {
    super(context);
    this.shouldReturnLocalSystem = returnLocalSystem;
  }

  protected async getLocalSystem(): Promise<JTAGSystem | null> {
    if (!this.shouldReturnLocalSystem) {
      return null;
    }
    
    if (!this.mockLocalSystem) {
      this.mockLocalSystem = await MockJTAGSystem.connect();
    }
    return this.mockLocalSystem as any;
  }

  protected async getTransportFactory(): Promise<any> {
    return {
      createTransport: async () => ({
        name: 'MockTransport',
        send: async () => ({ success: true }),
        disconnect: async () => {},
        isConnected: () => true,
        setMessageHandler: () => {}
      })
    };
  }

  // Expose protected method for testing
  public async testInitialize(options?: JTAGClientConnectOptions): Promise<void> {
    return this.initialize(options);
  }

  // Expose protected method for testing
  public testCreateLocalConnection(): JTAGConnection {
    return this.createLocalConnection();
  }

  // Expose discovered commands for testing
  public getDiscoveredCommands(): Map<string, CommandSignature> {
    return this.discoveredCommands;
  }
}

function testJTAGClientConstruction() {
  console.log('  📝 Testing JTAG client construction...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const context: JTAGContext = { uuid: 'test-client', environment: 'server' };
      const client = new TestJTAGClient(context);
      
      // Test basic properties
      if (client.context.uuid !== 'test-client') {
        reject(new Error('Client should preserve context'));
        return;
      }
      
      if (client.sessionId !== 'test-client') {
        reject(new Error('Client should set sessionId from context'));
        return;
      }
      
      if (client.context.environment !== 'server') {
        reject(new Error('Client should preserve environment'));
        return;
      }
      
      console.log('  ✅ JTAG client construction works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testLocalSystemDetection() {
  console.log('  📝 Testing local system detection...');
  
  return new Promise<void>((resolve, reject) => {
    const context: JTAGContext = { uuid: 'test-local', environment: 'server' };
    
    // Test with local system available
    const localClient = new TestJTAGClient(context, true);
    localClient.testInitialize().then(async () => {
      // Verify local connection was created
      if (!localClient.systemInstance) {
        reject(new Error('Local system should be available'));
        return;
      }
      
      console.log('  ✅ Local system detection works');
      resolve();
    }).catch(reject);
  });
}

function testRemoteSystemFallback() {
  console.log('  📝 Testing remote system fallback...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const context: JTAGContext = { uuid: 'test-remote-fallback', environment: 'server' };
      
      // Test client setup for remote (no local system)
      const remoteClient = new TestJTAGClient(context, false);
      
      // Test that getLocalSystem returns null for remote setup
      remoteClient.getLocalSystem().then((localSystem) => {
        if (localSystem !== null) {
          reject(new Error('Remote client should not have local system'));
          return;
        }
        
        console.log('  ✅ Remote system fallback works');
        resolve();
      }).catch(reject);
    } catch (error) {
      reject(error);
    }
  });
}

function testSessionManagement() {
  console.log('  📝 Testing session management...');
  
  return new Promise<void>((resolve, reject) => {
    // Test bootstrap session replacement
    const bootstrapContext: JTAGContext = { 
      uuid: SYSTEM_SCOPES.UNKNOWN_SESSION, 
      environment: 'server' 
    };
    
    const client = new TestJTAGClient(bootstrapContext);
    
    client.testInitialize().then(() => {
      // Check if session was replaced by SessionDaemon
      if (client.sessionId === SYSTEM_SCOPES.UNKNOWN_SESSION) {
        reject(new Error('Bootstrap session should be replaced by SessionDaemon'));
        return;
      }
      
      if (!client.sessionId.startsWith('real-session-')) {
        reject(new Error('Session should be replaced with real session from daemon'));
        return;
      }
      
      if (client.context.uuid === SYSTEM_SCOPES.UNKNOWN_SESSION) {
        reject(new Error('Context UUID should be updated with new session'));
        return;
      }
      
      console.log('  ✅ Session management works');
      resolve();
    }).catch(reject);
  });
}

function testLocalConnection() {
  console.log('  📝 Testing local connection logic...');
  
  return new Promise<void>((resolve, reject) => {
    const context: JTAGContext = { uuid: 'test-local-conn', environment: 'server' };
    const client = new TestJTAGClient(context);
    
    client.testInitialize().then(() => {
      // Test local connection creation
      const connection = client.testCreateLocalConnection();
      
      if (!(connection instanceof LocalConnection)) {
        reject(new Error('Should create LocalConnection instance'));
        return;
      }
      
      if (connection.sessionId !== client.sessionId) {
        reject(new Error('Connection should use client session ID'));
        return;
      }
      
      if (connection.context.environment !== 'server') {
        reject(new Error('Connection should preserve environment'));
        return;
      }
      
      // Test command execution through local connection
      return connection.executeCommand('ping', { message: 'test' });
    }).then((result: any) => {
      if (!result.success || result.message !== 'pong') {
        reject(new Error('Local connection should execute commands'));
        return;
      }
      
      console.log('  ✅ Local connection logic works');
      resolve();
    }).catch(reject);
  });
}

function testRemoteConnection() {
  console.log('  📝 Testing remote connection logic...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const context: JTAGContext = { uuid: 'test-remote-conn', environment: 'browser' };
      const client = new TestJTAGClient(context, false); // Force remote connection
      
      // Test remote connection creation  
      const connection = client.createRemoteConnection();
      
      if (!(connection instanceof RemoteConnection)) {
        reject(new Error('Should create RemoteConnection instance'));
        return;
      }
      
      if (connection.sessionId !== client.sessionId) {
        reject(new Error('Remote connection should use client session ID'));
        return;
      }
      
      if (connection.context.environment !== 'browser') {
        reject(new Error('Remote connection should preserve environment'));
        return;
      }
      
      // Test CommandsInterface (should be empty for remote until discovery)
      const commandsInterface = connection.getCommandsInterface();
      if (!(commandsInterface instanceof Map)) {
        reject(new Error('Remote connection should return Map for commands interface'));
        return;
      }
      
      console.log('  ✅ Remote connection logic works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testCommandDiscovery() {
  console.log('  📝 Testing command discovery...');
  
  return new Promise<void>((resolve, reject) => {
    const context: JTAGContext = { uuid: 'test-discovery', environment: 'server' };
    const client = new TestJTAGClient(context);
    
    client.testInitialize().then(() => {
      // Check discovered commands
      const discoveredCommands = client.getDiscoveredCommands();
      
      if (discoveredCommands.size === 0) {
        reject(new Error('Commands should be discovered during initialization'));
        return;
      }
      
      if (!discoveredCommands.has('ping')) {
        reject(new Error('Ping command should be discovered'));
        return;
      }
      
      if (!discoveredCommands.has('screenshot')) {
        reject(new Error('Screenshot command should be discovered'));
        return;
      }
      
      console.log('  ✅ Command discovery works');
      resolve();
    }).catch(reject);
  });
}

function testDynamicCommandsInterface() {
  console.log('  📝 Testing dynamic commands interface...');
  
  return new Promise<void>((resolve, reject) => {
    const context: JTAGContext = { uuid: 'test-commands', environment: 'server' };
    const client = new TestJTAGClient(context);
    
    client.testInitialize().then(() => {
      // Test list command (always available)
      return client.commands.list();
    }).then((listResult: ListResult) => {
      if (!listResult.success) {
        reject(new Error('List command should succeed'));
        return;
      }
      
      if (listResult.totalCount !== 3) {
        reject(new Error('List should return 3 commands'));
        return;
      }
      
      // Test discovered command
      return client.commands.ping({ message: 'test' });
    }).then((pingResult: any) => {
      if (!pingResult.success || pingResult.message !== 'pong') {
        reject(new Error('Ping command should work through dynamic interface'));
        return;
      }
      
      // Test undiscovered command (should throw)
      try {
        client.commands.unknownCommand();
        reject(new Error('Unknown command should throw error'));
        return;
      } catch (error) {
        // Expected error
      }
      
      console.log('  ✅ Dynamic commands interface works');
      resolve();
    }).catch(reject);
  });
}

function testClientEnvironmentSpecifics() {
  console.log('  📝 Testing client environment specifics...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      // Test browser client
      const browserContext: JTAGContext = { uuid: 'test-browser', environment: 'browser' };
      const browserClient = new JTAGClientBrowser(browserContext);
      
      if (browserClient.context.environment !== 'browser') {
        reject(new Error('Browser client should have browser environment'));
        return;
      }
      
      // Test server client
      const serverContext: JTAGContext = { uuid: 'test-server', environment: 'server' };
      const serverClient = new JTAGClientServer(serverContext);
      
      if (serverClient.context.environment !== 'server') {
        reject(new Error('Server client should have server environment'));
        return;
      }
      
      console.log('  ✅ Client environment specifics work');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testConnectionAbstraction() {
  console.log('  📝 Testing connection abstraction...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const context: JTAGContext = { uuid: 'test-abstraction', environment: 'server' };
      
      // Test LocalConnection
      const mockSystem = new MockJTAGSystem(context);
      const localConnection = new LocalConnection(mockSystem as any, context, 'test-session');
      
      // Test connection properties
      if (localConnection.sessionId !== 'test-session') {
        reject(new Error('LocalConnection should preserve session ID'));
        return;
      }
      
      if (localConnection.context.environment !== 'server') {
        reject(new Error('LocalConnection should preserve context'));
        return;
      }
      
      // Test commands interface delegation
      const commandsInterface = localConnection.getCommandsInterface();
      if (!commandsInterface.has('ping')) {
        reject(new Error('LocalConnection should delegate commands interface'));
        return;
      }
      
      // Test RemoteConnection
      const client = new TestJTAGClient(context, false);
      const remoteConnection = new RemoteConnection(client);
      
      if (remoteConnection.sessionId !== client.sessionId) {
        reject(new Error('RemoteConnection should use client session ID'));
        return;
      }
      
      console.log('  ✅ Connection abstraction works');
      resolve();
    } catch (error) {
      reject(error);
    }
  });
}

function testTransportHandlerInterface() {
  console.log('  📝 Testing transport handler interface...');
  
  return new Promise<void>((resolve, reject) => {
    try {
      const context: JTAGContext = { uuid: 'test-transport', environment: 'server' };
      const client = new TestJTAGClient(context);
      
      // Test transport ID
      if (client.transportId !== client.sessionId) {
        reject(new Error('Transport ID should match session ID'));
        return;
      }
      
      // Test transport message handling
      const testMessage: JTAGMessage = {
        messageType: 'event',
        context,
        origin: 'test',
        endpoint: 'test',
        payload: { data: 'test' },
        correlationId: 'test-id'
      };
      
      client.handleTransportMessage(testMessage).then((response) => {
        if (!response.success) {
          reject(new Error('Transport message handling should succeed'));
          return;
        }
        
        if (response.sessionId !== client.sessionId) {
          reject(new Error('Response should include client session ID'));
          return;
        }
        
        console.log('  ✅ Transport handler interface works');
        resolve();
      }).catch(reject);
    } catch (error) {
      reject(error);
    }
  });
}

// Run all tests
async function runAllTests() {
  try {
    await testJTAGClientConstruction();
    await testLocalSystemDetection();
    await testRemoteSystemFallback();
    await testSessionManagement();
    await testLocalConnection();
    await testRemoteConnection();
    await testCommandDiscovery();
    await testDynamicCommandsInterface();
    await testClientEnvironmentSpecifics();
    await testConnectionAbstraction();
    await testTransportHandlerInterface();
    
    console.log('✅ All client connection logic unit tests passed!');
    console.log('\\n📋 TEST SUMMARY:');
    console.log('  ✅ JTAG client construction and context management');
    console.log('  ✅ Local vs remote system detection and fallback');
    console.log('  ✅ Session management and bootstrap protocol');
    console.log('  ✅ Local connection with direct system calls');
    console.log('  ✅ Remote connection with transport routing');
    console.log('  ✅ Dynamic command discovery via list command');
    console.log('  ✅ Dynamic commands interface with proxy pattern');
    console.log('  ✅ Browser and server client specialization');
    console.log('  ✅ Connection abstraction and interface compliance');
    console.log('  ✅ Transport handler interface implementation');
    console.log('\\n🎯 Client connection logic is ready for integration testing!');
    
    process.exit(0);
  } catch (error) {
    console.error('❌ Client connection logic unit test failed:', error);
    process.exit(1);
  }
}

runAllTests();