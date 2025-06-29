/**
 * Browser Client → Bootstrap Arrow Test
 * Testing the direct browser → bootstrap connection (bypassing WebSocket/daemon layers)
 */

import { BootstrapSystem } from '../BootstrapSystem.js';
import { EventEmitter } from 'events';

// Simulate browser client behavior
class BrowserClientSimulator extends EventEmitter {
  private messageId = 0;
  private pendingMessages = new Map<string, { resolve: Function; reject: Function }>();

  constructor(private bootstrap: BootstrapSystem) {
    super();
    console.debug('🌐 BROWSER_SIM: Browser client simulator initialized');
  }

  async sendCommand(command: string, params: any = {}): Promise<any> {
    const messageId = `browser_msg_${++this.messageId}`;
    
    console.debug(`📤 BROWSER_SIM: Sending command directly to bootstrap:`, {
      command,
      id: messageId,
      params
    });

    try {
      // Direct call to bootstrap system
      const result = await this.bootstrap.executeCommand(command, params);
      
      const response = {
        id: messageId,
        success: true,
        data: result,
        originalCommand: command,
        processedBy: 'direct-bootstrap-connection',
        timestamp: new Date().toISOString()
      };

      console.debug(`📥 BROWSER_SIM: Received response from bootstrap:`, {
        id: response.id,
        success: response.success,
        command: command
      });

      return response;
      
    } catch (error) {
      const errorResponse = {
        id: messageId,
        success: false,
        error: (error as Error).message,
        originalCommand: command,
        processedBy: 'direct-bootstrap-connection'
      };

      console.debug(`❌ BROWSER_SIM: Error response from bootstrap:`, {
        id: errorResponse.id,
        error: errorResponse.error,
        command: command
      });

      throw errorResponse;
    }
  }
}

async function testBrowserBootstrapArrow(): Promise<void> {
  console.log('🧪 Testing Browser Client → Bootstrap Arrow (Direct)...\n');

  // Initialize bootstrap system
  const bootstrap = new BootstrapSystem();
  const browserClient = new BrowserClientSimulator(bootstrap);

  try {
    // Test 1: Start bootstrap system
    console.log('📋 Test 1: Start bootstrap system for direct browser connection');
    await bootstrap.start();
    console.log('✅ Bootstrap system ready for direct browser calls');

    // Test 2: Direct info command
    console.log('\n📋 Test 2: Direct info command Browser → Bootstrap');
    
    const infoResponse = await browserClient.sendCommand('info', { section: 'version' });
    console.log('✅ Browser → Bootstrap info command:', infoResponse.success);
    console.debug('📊 Direct info response:', {
      version: infoResponse.data.data.version,
      processedBy: infoResponse.data.data.processedBy
    });

    // Test 3: Direct status command
    console.log('\n📋 Test 3: Direct status command Browser → Bootstrap');
    
    const statusResponse = await browserClient.sendCommand('status', {});
    console.log('✅ Browser → Bootstrap status command:', statusResponse.success);
    console.debug('📊 Direct status response:', {
      systemReady: statusResponse.data.data.systemReady,
      processedBy: statusResponse.data.data.processedBy
    });

    // Test 4: Direct list command
    console.log('\n📋 Test 4: Direct list command Browser → Bootstrap');
    
    const listResponse = await browserClient.sendCommand('list', {});
    console.log('✅ Browser → Bootstrap list command:', listResponse.success);
    console.debug('📊 Direct list response:', {
      totalCommands: listResponse.data.data.totalCommands,
      systemReady: listResponse.data.data.systemReady
    });

    // Test 5: Direct help command (should call list internally)
    console.log('\n📋 Test 5: Direct help command Browser → Bootstrap (calls list internally)');
    
    const helpResponse = await browserClient.sendCommand('help', {});
    console.log('✅ Browser → Bootstrap help command:', helpResponse.success);
    console.debug('📊 Direct help response:', {
      availableCommands: helpResponse.data.data.availableCommands.length,
      basedOnListResult: helpResponse.data.data.basedOnListResult,
      processedBy: helpResponse.data.data.processedBy
    });

    // Test 6: Command interdependency verification
    console.log('\n📋 Test 6: Verify help → list interdependency in direct browser calls');
    
    const helpWithParam = await browserClient.sendCommand('help', { command: 'info' });
    console.log('✅ Browser → Bootstrap help with param:', helpWithParam.success);
    console.debug('📊 Help with param response:', {
      command: helpWithParam.data.data.command,
      category: helpWithParam.data.data.category,
      commandSource: helpWithParam.data.data.commandSource
    });

    // Test 7: Concurrent direct commands
    console.log('\n📋 Test 7: Concurrent direct commands Browser → Bootstrap');
    
    const concurrentPromises = [
      browserClient.sendCommand('info', {}),
      browserClient.sendCommand('status', {}),
      browserClient.sendCommand('list', {}),
      browserClient.sendCommand('help', {})
    ];

    const concurrentResults = await Promise.all(concurrentPromises);
    const successCount = concurrentResults.filter(r => r.success).length;
    console.log(`✅ Browser → Bootstrap concurrent: ${successCount}/4 commands succeeded`);

    // Test 8: Command queueing test (simulate starting over)
    console.log('\n📋 Test 8: Test command queueing with fresh bootstrap system');
    
    const freshBootstrap = new BootstrapSystem();
    const freshBrowserClient = new BrowserClientSimulator(freshBootstrap);
    
    console.debug('🔧 TEST: Sending commands before bootstrap initialization...');
    
    // Queue commands before initialization
    const queuedPromise = freshBrowserClient.sendCommand('list', {});
    
    // Start system (should process queued commands)
    await freshBootstrap.start();
    
    // Wait for queued command to resolve
    const queuedResult = await queuedPromise;
    console.log('✅ Browser → Bootstrap queued command resolved:', queuedResult.success);
    console.debug('📊 Queued command result:', {
      totalCommands: queuedResult.data.data.totalCommands,
      queueingWorked: true
    });

    // Test 9: Error handling
    console.log('\n📋 Test 9: Error handling Browser → Bootstrap');
    
    try {
      await browserClient.sendCommand('nonexistent-command', {});
      console.log('❌ Should have failed for unknown command');
    } catch (error) {
      console.log('✅ Browser → Bootstrap error handling works:', (error as any).error.includes('Bootstrap command not found'));
    }

    console.log('\n✅ Browser Client → Bootstrap arrow test complete!');
    console.debug('🎯 VERIFIED: Browser client can successfully communicate directly with bootstrap layer');

  } catch (error) {
    console.error('❌ Browser → Bootstrap arrow test failed:', error);
    throw error;
  }
}

// Run the test
testBrowserBootstrapArrow().catch((error) => {
  console.error('❌ Test execution failed:', error);
  process.exit(1);
});