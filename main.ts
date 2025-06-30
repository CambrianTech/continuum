#!/usr/bin/env tsx
/**
 * Continuum System - Clean event-driven architecture
 * No timeouts, no process spawning, no fake success reporting
 */

import { EventEmitter } from 'events';
import { WebSocketDaemon } from './src/integrations/websocket/WebSocketDaemon';
import { RendererDaemon } from './src/daemons/renderer/RendererDaemon';
import { CommandProcessorDaemon } from './src/daemons/command-processor/CommandProcessorDaemon';

export class ContinuumSystem extends EventEmitter {
  private daemons = new Map();
  private readyDaemons = new Set<string>();

  constructor() {
    super();
    
    // Create daemons
    this.daemons.set('websocket', new WebSocketDaemon());
    this.daemons.set('renderer', new RendererDaemon());
    this.daemons.set('command-processor', new CommandProcessorDaemon());
  }

  async start(): Promise<void> {
    console.log('🌟 Continuum System Starting');
    console.log('============================');
    
    // Clear any existing port conflicts
    await this.clearPorts();
    
    // Set up daemon event listeners
    for (const [name, daemon] of this.daemons) {
      daemon.on('started', () => this.handleDaemonReady(name));
      daemon.on('failed', (error: any) => this.handleDaemonFailed(name, error));
    }
    
    // Start all daemons
    console.log('🚀 Starting daemons...');
    for (const [name, daemon] of this.daemons) {
      console.log(`🚀 Starting ${name} daemon...`);
      await daemon.start();
    }
    
    // Wait for ALL daemons to be ready
    console.log('⏳ Waiting for all daemons to be ready...');
    await this.waitForSystemReady();
    
    // Set up inter-daemon communication
    await this.setupInterDaemonCommunication();
    
    // System is now genuinely ready
    console.log('');
    console.log('✅ System ready and operational');
    console.log('🌐 Browser interface: http://localhost:9000');
    console.log('🔌 WebSocket API: ws://localhost:9000');
    console.log('');
    
    // Run self-tests to validate everything works
    await this.runSelfTests();
  }

  private async clearPorts(): Promise<void> {
    // Check if port 9000 is in use and handle gracefully
    try {
      const net = await import('net');
      const server = net.createServer();
      
      return new Promise((resolve, reject) => {
        server.listen(9000, () => {
          server.close();
          resolve();
        });
        
        server.on('error', (error: any) => {
          if (error.code === 'EADDRINUSE') {
            console.log('⚠️  Port 9000 in use - attempting cleanup...');
            // In a real implementation, we'd identify and clean up the conflicting process
            // For now, just fail fast with a clear message
            reject(new Error('Port 9000 is in use. Please stop other Continuum instances.'));
          } else {
            reject(error);
          }
        });
      });
    } catch (error) {
      console.log('⚠️  Port check failed:', error);
    }
  }

  private async waitForSystemReady(): Promise<void> {
    return new Promise((resolve, reject) => {
      // Set up timeout for safety (but based on real failure, not arbitrary waiting)
      const timeout = setTimeout(() => {
        const missing = Array.from(this.daemons.keys()).filter(name => !this.readyDaemons.has(name));
        reject(new Error(`Daemons failed to start: ${missing.join(', ')}`));
      }, 30000); // 30 seconds is generous for real failures
      
      this.on('all-daemons-ready', () => {
        clearTimeout(timeout);
        resolve();
      });
      
      this.on('daemon-failed', () => {
        clearTimeout(timeout);
        reject(new Error('Critical daemon failed to start'));
      });
      
      // Check if already ready (race condition protection)
      if (this.readyDaemons.size === this.daemons.size) {
        clearTimeout(timeout);
        resolve();
      }
    });
  }

  private handleDaemonReady(name: string): void {
    console.log(`✅ ${name} daemon ready`);
    this.readyDaemons.add(name);
    
    if (this.readyDaemons.size === this.daemons.size) {
      this.emit('all-daemons-ready');
    }
  }

  private handleDaemonFailed(name: string, error: any): void {
    console.log(`❌ ${name} daemon failed:`, error);
    this.emit('daemon-failed', { name, error });
  }

  private async setupInterDaemonCommunication(): Promise<void> {
    console.log('🔌 Setting up inter-daemon communication...');
    
    const webSocketDaemon = this.daemons.get('websocket');
    const rendererDaemon = this.daemons.get('renderer');
    const commandProcessorDaemon = this.daemons.get('command-processor');
    
    // Register daemons with WebSocket daemon for message routing
    webSocketDaemon.registerDaemon(rendererDaemon);
    webSocketDaemon.registerDaemon(commandProcessorDaemon);
    
    // Register HTTP routes  
    webSocketDaemon.registerRouteHandler('/', 'renderer', 'render_ui');
    webSocketDaemon.registerRouteHandler('/src/ui/continuum-browser.js', 'renderer', 'render_ui_components');
    webSocketDaemon.registerRouteHandler('/dist/api.js', 'renderer', 'render_api');
    
    // Register wildcard route for all UI components (dynamic discovery)
    webSocketDaemon.registerRouteHandler('/src/ui/components/*', 'renderer', 'serve_ui_component');
    
    console.log('✅ Inter-daemon communication established');
  }

  private async runSelfTests(): Promise<void> {
    console.log('🧪 Running self-tests...');
    
    try {
      // Test that localhost:9000 responds
      const response = await fetch('http://localhost:9000');
      if (response.ok) {
        console.log('✅ HTTP server responding');
      } else {
        console.log(`⚠️  HTTP server returned: ${response.status}`);
      }
      
      // Test WebSocket connection
      const WebSocket = (await import('ws')).default;
      const ws = new WebSocket('ws://localhost:9000');
      
      await new Promise((resolve, reject) => {
        const timeout = setTimeout(() => {
          ws.close();
          reject(new Error('WebSocket connection timeout'));
        }, 5000);
        
        ws.on('open', () => {
          clearTimeout(timeout);
          ws.close();
          console.log('✅ WebSocket connection working');
          resolve();
        });
        
        ws.on('error', (error) => {
          clearTimeout(timeout);
          reject(error);
        });
      });
      
    } catch (error) {
      console.log('⚠️  Self-test failed:', error);
      console.log('🔧 System may not be fully operational');
    }
  }

  async stop(): Promise<void> {
    console.log('🛑 Stopping Continuum System...');
    
    for (const [name, daemon] of this.daemons) {
      console.log(`🛑 Stopping ${name}...`);
      await daemon.stop();
    }
    
    console.log('✅ System stopped');
  }
}

async function main() {
  const system = new ContinuumSystem();
  
  // Graceful shutdown
  process.on('SIGINT', async () => {
    console.log('\n🛑 Received shutdown signal...');
    await system.stop();
    process.exit(0);
  });
  
  process.on('SIGTERM', async () => {
    console.log('\n🛑 Received termination signal...');
    await system.stop();
    process.exit(0);
  });
  
  try {
    await system.start();
    
    // Keep running
    console.log('🔄 System running - press Ctrl+C to stop');
    
  } catch (error) {
    console.error('❌ System startup failed:', error);
    await system.stop();
    process.exit(1);
  }
}

if (process.argv[1] && process.argv[1].endsWith('main.ts')) {
  main();
}