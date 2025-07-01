#!/usr/bin/env tsx
/**
 * Continuum Core System
 * Main entry point for the Continuum daemon ecosystem
 */

import { SimpleDaemonStarter } from './SimpleDaemonStarter.js';

export interface ContinuumSystem {
  api: {
    http: string;
    websocket: string;
    health: string;
  };
  jtag: {
    console: {
      capture: () => Promise<any>;
      logs: () => Promise<any>;
    };
    commands: {
      execute: (command: string, args?: any) => Promise<any>;
      list: () => Promise<any>;
    };
    browser: {
      screenshot: () => Promise<any>;
      navigate: (url: string) => Promise<any>;
    };
    daemons: {
      list: () => Promise<any>;
      restart: (daemon: string) => Promise<any>;
    };
  };
  events: {
    on: (event: string, callback: Function) => void;
    emit: (event: string, data: any) => void;
  };
  system: {
    status: () => Promise<any>;
    restart: () => Promise<ContinuumSystem>;
    stop: () => Promise<void>;
  };
}

export class Continuum {
  private daemonStarter: SimpleDaemonStarter;
  private isRunning: boolean = false;

  constructor() {
    this.daemonStarter = new SimpleDaemonStarter();
  }

  /**
   * Initialize and start the Continuum system
   * Returns hooks and API connection for JTAG integration
   */
  async init(_options: { devtools?: boolean } = {}): Promise<ContinuumSystem> {
    if (this.isRunning) {
      console.log('✅ Continuum is already running');
      return this.getSystemHooks();
    }

    console.log('🌟 Initializing Continuum System');
    console.log('🐳 Like Docker Desktop for AI collaboration');
    console.log('');

    try {
      await this.daemonStarter.startAll();
      this.isRunning = true;
      
      // Auto-launch browser (BrowserManager daemon handles single window)
      setTimeout(async () => {
        console.log('🌐 Opening browser interface...');
        await this.openBrowser();
      }, 2000);
      
      console.log('✅ Continuum system ready');
      console.log('🌐 Interface: http://localhost:9000');
      console.log('🔌 API: ws://localhost:9000');
      
      return this.getSystemHooks();
      
    } catch (error) {
      console.error('❌ Failed to initialize Continuum:', error);
      throw error;
    }
  }

  /**
   * Connect to running Continuum system
   * Returns system hooks for JTAG/API integration
   */
  async connect(): Promise<ContinuumSystem> {
    try {
      // Check if already running
      const fetch = (await import('node-fetch')).default;
      const response = await fetch('http://localhost:9000/health');
      
      if (response.ok) {
        console.log('✅ Connected to running Continuum system');
        this.isRunning = true;
        await this.openBrowser();
        return this.getSystemHooks();
      }
    } catch (error) {
      // Not running, start it
    }
    
    console.log('🚀 Continuum not running - starting now...');
    return await this.init();
  }

  /**
   * Get system hooks for JTAG autonomous development
   */
  private getSystemHooks(): ContinuumSystem {
    return {
      // API connections
      api: {
        http: 'http://localhost:9000',
        websocket: 'ws://localhost:9000',
        health: 'http://localhost:9000/health'
      },
      
      // JTAG hooks for autonomous development
      jtag: {
        console: this.getConsoleHook(),
        commands: this.getCommandHook(),
        browser: this.getBrowserHook(),
        daemons: this.getDaemonHook()
      },
      
      // Event system
      events: {
        on: (_event: string, _callback: Function) => {
          // TODO: Implement event system
        },
        emit: (_event: string, _data: any) => {
          // TODO: Implement event system
        }
      },
      
      // System control
      system: {
        status: () => this.status(),
        restart: () => this.restart(),
        stop: () => this.stop()
      }
    };
  }

  private getConsoleHook() {
    return {
      capture: async () => {
        // Connect to console capture WebSocket
        const fetch = (await import('node-fetch')).default;
        return fetch('http://localhost:9000/api/console/connect');
      },
      logs: async () => {
        const fetch = (await import('node-fetch')).default;
        return fetch('http://localhost:9000/api/console/logs');
      }
    };
  }

  private getCommandHook() {
    return {
      execute: async (command: string, args: any = {}) => {
        const fetch = (await import('node-fetch')).default;
        return fetch('http://localhost:9000/api/command', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ command, args })
        });
      },
      list: async () => {
        const fetch = (await import('node-fetch')).default;
        return fetch('http://localhost:9000/api/commands');
      }
    };
  }

  private getBrowserHook() {
    return {
      screenshot: async () => {
        const fetch = (await import('node-fetch')).default;
        return fetch('http://localhost:9000/api/browser/screenshot');
      },
      navigate: async (url: string) => {
        const fetch = (await import('node-fetch')).default;
        return fetch('http://localhost:9000/api/browser/navigate', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ url })
        });
      }
    };
  }

  private getDaemonHook() {
    return {
      list: async () => {
        const fetch = (await import('node-fetch')).default;
        return fetch('http://localhost:9000/api/daemons');
      },
      restart: async (daemon: string) => {
        const fetch = (await import('node-fetch')).default;
        return fetch(`http://localhost:9000/api/daemon/${daemon}/restart`, {
          method: 'POST'
        });
      }
    };
  }

  /**
   * Restart the system
   */
  async restart(): Promise<ContinuumSystem> {
    await this.stop();
    await new Promise(resolve => setTimeout(resolve, 2000));
    return await this.init();
  }

  /**
   * Stop the Continuum system
   */
  async stop(): Promise<void> {
    if (!this.isRunning) {
      console.log('📍 Continuum is not running');
      return;
    }

    console.log('🛑 Stopping Continuum system...');
    await this.daemonStarter.stopAll();
    this.isRunning = false;
    console.log('✅ Continuum stopped');
  }

  /**
   * Get system status
   */
  async status(): Promise<any> {
    try {
      const fetch = (await import('node-fetch')).default;
      const response = await fetch('http://localhost:9000/health');
      
      if (response.ok) {
        const health = await response.json();
        return {
          running: true,
          health,
          interface: 'http://localhost:9000',
          api: 'ws://localhost:9000'
        };
      }
    } catch (error) {
      // System not running
    }
    
    return {
      running: false,
      message: 'Continuum system is not running'
    };
  }

  /**
   * Open browser interface (delegates to BrowserManager daemon)
   */
  private async openBrowser(): Promise<void> {
    const { spawn } = (await import('child_process'));
    let command;
    
    if (process.platform === 'darwin') {
      command = 'open';
    } else if (process.platform === 'win32') {
      command = 'start';
    } else {
      command = 'xdg-open';
    }
    
    try {
      spawn(command, ['http://localhost:9000'], { 
        stdio: 'ignore', 
        detached: true 
      }).unref();
    } catch (error) {
      console.log('💡 Please open: http://localhost:9000');
    }
  }

  /**
   * Keep process alive for CLI usage
   */
  keepAlive(): void {
    // Setup graceful shutdown
    process.on('SIGINT', async () => {
      console.log('\n🛑 Received shutdown signal...');
      await this.stop();
      process.exit(0);
    });
    
    process.on('SIGTERM', async () => {
      console.log('\n🛑 Received termination signal...');
      await this.stop();
      process.exit(0);
    });
    
    // Exit cleanly after successful launch
    console.log('✅ Continuum launched successfully');
    console.log('🌐 Browser interface: http://localhost:9000');
    console.log('🔄 Daemons running in background');
    
    // Give user control back
    setTimeout(() => {
      process.exit(0);
    }, 1000);
  }
}

// Export for use in CLI
export default Continuum;

// Direct execution support
if (import.meta.url === `file://${process.argv[1]}`) {
  const continuum = new Continuum();
  
  continuum.init()
    .then(() => continuum.keepAlive())
    .catch(error => {
      console.error('❌ Continuum startup failed:', error);
      process.exit(1);
    });
}