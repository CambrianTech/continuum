/**
 * Simple Daemon Starter
 * Manages the core Continuum daemon processes
 */

import { spawn, ChildProcess } from 'child_process';
import { promises as fs } from 'fs';
// path module available for future use

interface DaemonConfig {
  name: string;
  script: string;
  critical: boolean;
  process?: ChildProcess;
}

export class SimpleDaemonStarter {
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
    },
    {
      name: 'BrowserManager',
      script: 'src/daemons/browser-manager/BrowserManagerDaemon.ts',
      critical: false
    }
  ];

  private lockFile = '.continuum/system.lock';

  async startAll(): Promise<void> {
    console.log('🌟 Starting Continuum Daemon System');
    console.log('===================================');
    
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
    
    console.log('');
    console.log('✅ Core daemons started');
  }

  private async startDaemon(daemon: DaemonConfig): Promise<void> {
    console.log(`🚀 Starting ${daemon.name}...`);
    
    const childProcess = spawn('npx', ['tsx', daemon.script], {
      stdio: 'pipe',
      detached: false,
      cwd: process.cwd()
    });
    
    daemon.process = childProcess;
    
    // Handle process output with prefixed logging
    childProcess.stdout?.on('data', (data: any) => {
      const lines = data.toString().trim().split('\n');
      lines.forEach((line: string) => {
        if (line.trim()) {
          console.log(`[${daemon.name}] ${line}`);
        }
      });
    });
    
    childProcess.stderr?.on('data', (data: any) => {
      const lines = data.toString().trim().split('\n');
      lines.forEach((line: string) => {
        if (line.trim()) {
          console.log(`[${daemon.name}] ❌ ${line}`);
        }
      });
    });
    
    // Handle process exit
    childProcess.on('exit', (code: any, signal: any) => {
      console.log(`[${daemon.name}] Process exited with code ${code}, signal ${signal}`);
      if (daemon.critical && code !== 0) {
        console.log(`❌ Critical daemon ${daemon.name} failed - system unstable`);
      }
    });
    
    // Wait for startup
    await new Promise(resolve => setTimeout(resolve, 1000));
    console.log(`✅ ${daemon.name} started`);
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