#!/usr/bin/env tsx
/**
 * Continuum Daemon System Starter
 * Simple orchestration of independent TypeScript daemons
 */

import { spawn, ChildProcess } from 'child_process';
import { promises as fs } from 'fs';
import path from 'path';

interface DaemonConfig {
  name: string;
  script: string;
  critical: boolean;
  restartCount?: number;
  process?: ChildProcess;
}

class SimpleDaemonStarter {
  private daemons: DaemonConfig[] = [
    {
      name: 'WebSocket',
      script: 'src/integrations/websocket/WebSocketDaemon.ts',
      critical: true
    },
    {
      name: 'CommandProcessor', 
      script: 'src/daemons/command-processor/CommandProcessorDaemon.ts',
      critical: true
    },
    {
      name: 'Renderer',
      script: 'src/daemons/renderer/RendererDaemon.ts', 
      critical: true
    }
  ];

  private lockFile = '.continuum/system.lock';

  async startAll(): Promise<void> {
    console.log('🌟 Continuum TypeScript Daemon System');
    console.log('====================================');
    
    // Create lock file directory
    await fs.mkdir('.continuum', { recursive: true });
    
    // Write lock file
    const lockData = {
      pid: process.pid,
      startTime: new Date().toISOString(),
      daemons: this.daemons.map(d => d.name)
    };
    await fs.writeFile(this.lockFile, JSON.stringify(lockData, null, 2));
    
    // Start each daemon
    for (const daemon of this.daemons) {
      await this.startDaemon(daemon);
    }
    
    // Wait for WebSocket daemon to be ready, then setup inter-daemon communication
    await this.setupInterDaemonCommunication();
    
    console.log('');
    console.log('✅ All daemons started successfully');
    console.log('🌐 Browser interface: http://localhost:9000');
    console.log('🔌 WebSocket API: ws://localhost:9000');
    console.log('');
  }

  private async setupInterDaemonCommunication(): Promise<void> {
    console.log('🔌 Setting up inter-daemon communication...');
    
    // Give daemons a moment to finish startup, then register routes
    await new Promise(resolve => setTimeout(resolve, 1000));
    
    // TODO: Replace with proper event-driven approach
    // For now, use a simple script approach to register RendererDaemon routes
    try {
      const { spawn } = await import('child_process');
      
      const registrationScript = spawn('npx', ['tsx', '-e', `
        const { RendererDaemon } = require('./src/daemons/renderer/RendererDaemon');
        const renderer = new RendererDaemon();
        renderer.registerRoutesWithWebSocket().then(() => {
          console.log('✅ Route registration complete');
          process.exit(0);
        }).catch(error => {
          console.error('❌ Route registration failed:', error);
          process.exit(1);
        });
      `], { stdio: 'pipe' });

      registrationScript.stdout?.on('data', (data) => {
        console.log(`[Registration] ${data.toString().trim()}`);
      });

      registrationScript.stderr?.on('data', (data) => {
        console.log(`[Registration] ❌ ${data.toString().trim()}`);
      });

      await new Promise((resolve) => {
        registrationScript.on('exit', resolve);
      });

    } catch (error) {
      console.log(`⚠️  Route registration failed: ${error}`);
    }
  }

  private async startDaemon(daemon: DaemonConfig): Promise<void> {
    console.log(`🚀 Starting ${daemon.name} daemon...`);
    
    const process = spawn('npx', ['tsx', daemon.script], {
      stdio: 'pipe',
      detached: false
    });
    
    daemon.process = process;
    daemon.restartCount = 0;
    
    // Handle process output
    process.stdout?.on('data', (data) => {
      const lines = data.toString().trim().split('\n');
      lines.forEach(line => {
        if (line.trim()) {
          console.log(`[${daemon.name}] ${line}`);
        }
      });
    });
    
    process.stderr?.on('data', (data) => {
      const lines = data.toString().trim().split('\n');
      lines.forEach(line => {
        if (line.trim()) {
          console.log(`[${daemon.name}] ❌ ${line}`);
        }
      });
    });
    
    // Handle process exit
    process.on('exit', (code, signal) => {
      console.log(`[${daemon.name}] Process exited with code ${code}, signal ${signal}`);
      if (daemon.critical && code !== 0) {
        console.log(`❌ Critical daemon ${daemon.name} failed - system may be unstable`);
      }
    });
    
    // Wait a moment for startup
    await new Promise(resolve => setTimeout(resolve, 1000));
    console.log(`✅ ${daemon.name} daemon started`);
  }

  async stopAll(): Promise<void> {
    console.log('🛑 Stopping all daemons...');
    
    for (const daemon of this.daemons) {
      if (daemon.process) {
        console.log(`🛑 Stopping ${daemon.name}...`);
        daemon.process.kill('SIGTERM');
      }
    }
    
    // Clean up lock file
    try {
      await fs.unlink(this.lockFile);
    } catch (error) {
      // Lock file may not exist
    }
    
    console.log('✅ All daemons stopped');
  }
}

async function main() {
  const starter = new SimpleDaemonStarter();
  
  // Setup graceful shutdown
  process.on('SIGINT', async () => {
    console.log('\n🛑 Received shutdown signal...');
    await starter.stopAll();
    process.exit(0);
  });
  
  process.on('SIGTERM', async () => {
    console.log('\n🛑 Received termination signal...');
    await starter.stopAll();
    process.exit(0);
  });
  
  try {
    await starter.startAll();
    
    // Keep process alive
    console.log('🔄 System running - press Ctrl+C to stop');
    
    // Simple keep-alive without polling spam
    setInterval(() => {
      // Just keep the process alive
    }, 30000);
    
  } catch (error) {
    console.error('❌ Failed to start daemon system:', error);
    process.exit(1);
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch(error => {
    console.error('❌ Daemon system startup failed:', error);
    process.exit(1);
  });
}