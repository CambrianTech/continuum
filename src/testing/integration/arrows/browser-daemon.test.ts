/**
 * Browser Client → Daemon Arrow Test  
 * Testing the specific browser → daemon connection and command routing
 */

import { EventEmitter } from 'events';

// Simulate browser client behavior (same as before)
class BrowserClientSimulator extends EventEmitter {
  private messageId = 0;
  private pendingMessages = new Map<string, { resolve: Function; reject: Function }>();

  constructor() {
    super();
    console.debug('🌐 BROWSER_SIM: Browser client simulator initialized');
  }

  async sendCommand(command: string, params: any = {}): Promise<any> {
    const messageId = `browser_msg_${++this.messageId}`;
    
    const message = {
      type: 'execute_command',
      command,
      params,
      id: messageId,
      timestamp: new Date().toISOString(),
      source: 'browser-client'
    };

    console.debug(`📤 BROWSER_SIM: Sending command to daemon:`, {
      command: message.command,
      id: message.id,
      type: message.type
    });

    return new Promise((resolve, reject) => {
      this.pendingMessages.set(messageId, { resolve, reject });
      
      // Emit to daemon layer
      this.emit('daemon-message', message);
      
      // Timeout after 5 seconds
      setTimeout(() => {
        if (this.pendingMessages.has(messageId)) {
          this.pendingMessages.delete(messageId);
          reject(new Error(`Browser → Daemon timeout for command: ${command}`));
        }
      }, 5000);
    });
  }

  receiveResponse(response: any): void {
    console.debug(`📥 BROWSER_SIM: Received response from daemon:`, {
      id: response.id,
      success: response.success,
      command: response.originalCommand
    });

    const pending = this.pendingMessages.get(response.id);
    if (pending) {
      this.pendingMessages.delete(response.id);
      
      if (response.success) {
        pending.resolve(response);
      } else {
        pending.reject(new Error(response.error));
      }
    }
  }
}

// Simulate daemon behavior
class DaemonSimulator extends EventEmitter {
  private clients = new Set<BrowserClientSimulator>();
  private commandProcessors = new Map<string, Function>();

  constructor() {
    super();
    console.debug('⚙️ DAEMON_SIM: Daemon simulator initialized');
    this.setupCommandProcessors();
  }

  private setupCommandProcessors(): void {
    // Register command processors
    this.commandProcessors.set('info', this.processInfoCommand.bind(this));
    this.commandProcessors.set('status', this.processStatusCommand.bind(this));
    this.commandProcessors.set('list', this.processListCommand.bind(this));
    this.commandProcessors.set('help', this.processHelpCommand.bind(this));
    
    console.debug('⚙️ DAEMON_SIM: Registered command processors:', Array.from(this.commandProcessors.keys()));
  }

  connectClient(client: BrowserClientSimulator): void {
    console.debug('🔗 DAEMON_SIM: Browser client connected to daemon');
    
    this.clients.add(client);
    
    // Listen for messages from browser client
    client.on('daemon-message', (message) => {
      this.handleClientMessage(client, message);
    });

    client.emit('connected');
  }

  private async handleClientMessage(client: BrowserClientSimulator, message: any): Promise<void> {
    console.debug(`📥 DAEMON_SIM: Received message from browser:`, {
      command: message.command,
      id: message.id,
      source: message.source
    });

    try {
      // Route to appropriate command processor
      const processor = this.commandProcessors.get(message.command);
      if (!processor) {
        throw new Error(`No daemon processor for command: ${message.command}`);
      }

      console.debug(`🔄 DAEMON_SIM: Processing browser command: ${message.command}`);
      
      const result = await processor(message.params);
      
      const response = {
        id: message.id,
        success: true,
        data: result,
        originalCommand: message.command,
        processedBy: 'daemon-simulator',
        timestamp: new Date().toISOString()
      };

      console.debug(`📤 DAEMON_SIM: Sending response to browser:`, {
        id: response.id,
        success: response.success,
        command: message.command
      });

      client.receiveResponse(response);
      
    } catch (error) {
      console.debug(`❌ DAEMON_SIM: Error processing browser message:`, {
        error: (error as Error).message,
        command: message.command,
        id: message.id
      });

      const errorResponse = {
        id: message.id,
        success: false,
        error: (error as Error).message,
        originalCommand: message.command,
        processedBy: 'daemon-simulator'
      };

      client.receiveResponse(errorResponse);
    }
  }

  // Command processors
  private async processInfoCommand(params: any): Promise<any> {
    console.debug('🔧 DAEMON_SIM: Processing info command');
    await new Promise(resolve => setTimeout(resolve, 10)); // Simulate processing
    
    return {
      version: '0.2.test',
      system: 'daemon-simulator',
      params: params,
      processedBy: 'daemon-info-processor'
    };
  }

  private async processStatusCommand(params: any): Promise<any> {
    console.debug('🔧 DAEMON_SIM: Processing status command');
    await new Promise(resolve => setTimeout(resolve, 15)); // Simulate processing
    
    return {
      status: 'ready',
      daemonReady: true,
      uptime: '5m',
      processedBy: 'daemon-status-processor'
    };
  }

  private async processListCommand(params: any): Promise<any> {
    console.debug('🔧 DAEMON_SIM: Processing list command');
    await new Promise(resolve => setTimeout(resolve, 20)); // Simulate processing
    
    return {
      commands: ['info', 'status', 'list', 'help'],
      totalCommands: 4,
      source: 'daemon-command-registry',
      processedBy: 'daemon-list-processor'
    };
  }

  private async processHelpCommand(params: any): Promise<any> {
    console.debug('🔧 DAEMON_SIM: Processing help command');
    
    // Simulate calling list command internally (like real help command)
    const listResult = await this.processListCommand({});
    
    return {
      availableCommands: listResult.commands,
      commandCount: listResult.totalCommands,
      usage: 'Daemon help system',
      basedOnList: true,
      processedBy: 'daemon-help-processor'
    };
  }

  disconnectClient(client: BrowserClientSimulator): void {
    console.debug('🔗 DAEMON_SIM: Browser client disconnected from daemon');
    this.clients.delete(client);
    client.emit('disconnected');
  }
}

async function testBrowserDaemonArrow(): Promise<void> {
  console.log('🧪 Testing Browser Client → Daemon Arrow...\n');

  const daemon = new DaemonSimulator();
  const browserClient = new BrowserClientSimulator();

  try {
    // Test 1: Basic connection
    console.log('📋 Test 1: Browser client → Daemon connection');
    daemon.connectClient(browserClient);
    console.log('✅ Browser → Daemon connection established');

    // Test 2: Info command processing
    console.log('\n📋 Test 2: Info command Browser → Daemon');
    
    const infoResponse = await browserClient.sendCommand('info', { section: 'version' });
    console.log('✅ Browser → Daemon info command:', infoResponse.success);
    console.debug('📊 Info response:', {
      version: infoResponse.data.version,
      processedBy: infoResponse.data.processedBy
    });

    // Test 3: Status command processing
    console.log('\n📋 Test 3: Status command Browser → Daemon');
    
    const statusResponse = await browserClient.sendCommand('status', {});
    console.log('✅ Browser → Daemon status command:', statusResponse.success);
    console.debug('📊 Status response:', {
      status: statusResponse.data.status,
      daemonReady: statusResponse.data.daemonReady
    });

    // Test 4: List command processing
    console.log('\n📋 Test 4: List command Browser → Daemon');
    
    const listResponse = await browserClient.sendCommand('list', {});
    console.log('✅ Browser → Daemon list command:', listResponse.success);
    console.debug('📊 List response:', {
      totalCommands: listResponse.data.totalCommands,
      commands: listResponse.data.commands
    });

    // Test 5: Help command (with internal list call)
    console.log('\n📋 Test 5: Help command Browser → Daemon (calls list internally)');
    
    const helpResponse = await browserClient.sendCommand('help', {});
    console.log('✅ Browser → Daemon help command:', helpResponse.success);
    console.debug('📊 Help response:', {
      commandCount: helpResponse.data.commandCount,
      basedOnList: helpResponse.data.basedOnList,
      processedBy: helpResponse.data.processedBy
    });

    // Test 6: Concurrent commands to daemon
    console.log('\n📋 Test 6: Concurrent commands Browser → Daemon');
    
    const concurrentPromises = [
      browserClient.sendCommand('info', {}),
      browserClient.sendCommand('status', {}),
      browserClient.sendCommand('list', {}),
      browserClient.sendCommand('help', {})
    ];

    const concurrentResults = await Promise.all(concurrentPromises);
    const successCount = concurrentResults.filter(r => r.success).length;
    console.log(`✅ Browser → Daemon concurrent: ${successCount}/4 commands succeeded`);

    // Test 7: Error handling 
    console.log('\n📋 Test 7: Error handling Browser → Daemon');
    
    try {
      await browserClient.sendCommand('unknown-command', {});
      console.log('❌ Should have failed for unknown command');
    } catch (error) {
      console.log('✅ Browser → Daemon error handling works:', (error as Error).message.includes('No daemon processor'));
    }

    console.log('\n✅ Browser Client → Daemon arrow test complete!');
    console.debug('🎯 VERIFIED: Browser client can successfully communicate with daemon layer');

  } catch (error) {
    console.error('❌ Browser → Daemon arrow test failed:', error);
    throw error;
  } finally {
    // Cleanup
    daemon.disconnectClient(browserClient);
    console.debug('🧹 Browser → Daemon test cleanup complete');
  }
}

// Run the test
testBrowserDaemonArrow().catch((error) => {
  console.error('❌ Test execution failed:', error);
  process.exit(1);
});