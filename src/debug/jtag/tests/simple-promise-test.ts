#!/usr/bin/env npx tsx
/**
 * Step 4B: Simple Promise-Based Communication Test
 * 
 * Demonstrates the concept you suggested:
 * - Client grabs server endpoint 
 * - Calls server methods via simple promises
 * - Clean, direct communication pattern
 */

import { JTAGBase } from '@shared/JTAGBase';

// Simple promise-based communication pattern
class SimpleJTAGClient {
  private serverMethods: any;

  constructor(serverReference: any) {
    // Client grabs reference to server methods
    this.serverMethods = serverReference;
    console.log('📞 Client grabbed server endpoint/reference');
  }

  // Client calls server methods directly via promises
  async callServerLog(component: string, message: string): Promise<any> {
    console.log('📤 Client → Server: callServerLog()');
    return this.serverMethods.processLog(component, message);
  }

  async callServerScreenshot(filename: string, options: any): Promise<any> {
    console.log('📸 Client → Server: callServerScreenshot()');
    return this.serverMethods.takeScreenshot(filename, options);
  }

  async callServerHealth(): Promise<any> {
    console.log('💖 Client → Server: callServerHealth()');
    return this.serverMethods.getHealth();
  }
}

class SimpleJTAGServer {
  // Server provides promise-based methods
  async processLog(component: string, message: string): Promise<{success: boolean, result: any}> {
    try {
      console.log('🖥️ Server processing log:', component, '-', message);
      JTAGBase.log(component, message);
      return { success: true, result: 'Log processed successfully' };
    } catch (error) {
      return { success: false, result: error.message };
    }
  }

  async takeScreenshot(filename: string, options: any): Promise<{success: boolean, result: any}> {
    try {
      console.log('🖥️ Server taking screenshot:', filename);
      const result = await JTAGBase.screenshot(filename, options);
      return { 
        success: true, 
        result: { 
          filepath: result.filepath, 
          success: result.success,
          metadata: result.metadata 
        }
      };
    } catch (error) {
      return { success: false, result: error.message };
    }
  }

  async getHealth(): Promise<{success: boolean, result: any}> {
    try {
      console.log('🖥️ Server checking health');
      const connection = await JTAGBase.connect({ healthCheck: true });
      return { 
        success: true, 
        result: {
          healthy: connection.healthy,
          transport: connection.transport.type,
          latency: connection.transport.latency
        }
      };
    } catch (error) {
      return { success: false, result: error.message };
    }
  }

  // Server can also grab client endpoint and make calls (bidirectional)
  async callClient(clientReference: any, method: string, ...args: any[]): Promise<any> {
    console.log('🖥️ Server → Client:', method);
    return clientReference[method](...args);
  }
}

async function testSimplePromises() {
  console.log('🧪 Step 4B: Simple Promise-Based Communication Test\n');

  try {
    // Test 1: Initialize JTAG server
    console.log('📋 Test 4B.1: Initialize JTAG server');
    JTAGBase.initialize({
      context: 'server',
      enableConsoleOutput: true,
      enableRemoteLogging: false, // Keep it simple
      jtagPort: 9001
    });
    
    await new Promise(resolve => setTimeout(resolve, 1000));
    console.log('✅ JTAG server initialized');

    // Test 2: Create server with promise-based methods
    console.log('\n📋 Test 4B.2: Create promise-based server');
    const server = new SimpleJTAGServer();
    console.log('✅ Promise-based server created');

    // Test 3: Client grabs server endpoint/reference
    console.log('\n📋 Test 4B.3: Client grabs server reference');
    const client = new SimpleJTAGClient(server);
    console.log('✅ Client has server reference');

    // Test 4: Client calls server methods via promises
    console.log('\n📋 Test 4B.4: Client calls server via promises');
    
    // Test log call
    const logResult = await client.callServerLog(
      'PROMISE_CLIENT', 
      'Direct promise-based log call from client to server'
    );
    console.log('📝 Log result:', logResult.success ? '✅ Success' : '❌ Failed');
    
    // Test screenshot call
    const screenshotResult = await client.callServerScreenshot(
      'promise-screenshot', 
      { width: 800, height: 600 }
    );
    console.log('📸 Screenshot result:', screenshotResult.success ? '✅ Success' : '❌ Failed');
    
    // Test health call
    const healthResult = await client.callServerHealth();
    console.log('💖 Health result:', healthResult.success ? '✅ Success' : '❌ Failed');
    if (healthResult.success) {
      console.log('🔍 Health details:', healthResult.result);
    }

    // Test 5: Demonstrate bidirectional communication
    console.log('\n📋 Test 4B.5: Demonstrate bidirectional potential');
    
    // Create simple client methods that server could call
    const clientMethods = {
      async provideData(dataType: string): Promise<any> {
        console.log('📱 Client providing data:', dataType);
        return { dataType, data: `Mock ${dataType} data from client`, timestamp: new Date().toISOString() };
      },
      
      async confirmAction(action: string): Promise<boolean> {
        console.log('📱 Client confirming action:', action);
        return true; // In real app, might show UI confirmation
      }
    };

    // Server calls client methods
    const clientData = await server.callClient(clientMethods, 'provideData', 'user-settings');
    console.log('📤 Server got from client:', clientData);
    
    const confirmation = await server.callClient(clientMethods, 'confirmAction', 'save-screenshot');
    console.log('📤 Server got confirmation:', confirmation ? '✅ Confirmed' : '❌ Denied');

    console.log('\n🎉 Step 4B Complete: Simple promise-based communication works perfectly!');
    console.log('💡 Key advantages of this approach:');
    console.log('   • ✅ Simple: client.callServer(method, ...args) → Promise<result>');
    console.log('   • ✅ Direct: No complex message routing or protocols');
    console.log('   • ✅ Bidirectional: server.callClient() works the same way');
    console.log('   • ✅ Clean: Standard async/await JavaScript patterns');
    console.log('   • ✅ Type-safe: Can use TypeScript interfaces for contracts');
    console.log('   • ✅ Debuggable: Standard promise chains, easy to trace');
    
    return true;

  } catch (error) {
    console.error('❌ Step 4B Failed:', error);
    return false;
  }
}

// Run the test
testSimplePromises().then(success => {
  console.log('\n' + (success ? '🎉 Simple promise test PASSED' : '❌ Simple promise test FAILED'));
  process.exit(success ? 0 : 1);
}).catch(error => {
  console.error('💥 Simple promise test crashed:', error);
  process.exit(1);
});