/**
 * Daemon Manager - Process-driven architecture controller
 */

const { spawn } = require('child_process');
const path = require('path');

class DaemonManager {
  constructor() {
    this.daemons = new Map();
    this.daemonConfigs = {
      'command-processor': {
        path: 'src/daemons/command-processor/CommandProcessorDaemon.ts',
        runtime: 'tsx',
        autoRestart: true,
        critical: true
      },
      'websocket': {
        path: 'src/daemons/websocket/WebSocketDaemon.ts', 
        runtime: 'tsx',
        autoRestart: true,
        critical: true
      }
    };
  }

  async startDaemon(name) {
    const config = this.daemonConfigs[name];
    if (!config) {
      throw new Error(`Unknown daemon: ${name}`);
    }

    console.log(`🚀 Starting daemon: ${name}`);
    
    const daemon = spawn('npx', ['tsx', config.path], {
      stdio: ['pipe', 'pipe', 'pipe', 'ipc'],
      cwd: process.cwd()
    });

    daemon.stdout.on('data', (data) => {
      console.log(`[${name}] ${data.toString().trim()}`);
    });

    daemon.stderr.on('data', (data) => {
      console.error(`[${name}] ${data.toString().trim()}`);
    });

    daemon.on('close', (code) => {
      console.log(`🔄 Daemon ${name} exited with code ${code}`);
      if (config.autoRestart && code !== 0) {
        console.log(`♻️ Auto-restarting daemon: ${name}`);
        setTimeout(() => this.startDaemon(name), 2000);
      }
    });

    this.daemons.set(name, { process: daemon, config });
    return daemon;
  }

  async startCriticalDaemons() {
    const critical = Object.entries(this.daemonConfigs)
      .filter(([name, config]) => config.critical)
      .map(([name]) => name);

    console.log(`🔥 Starting critical daemons: ${critical.join(', ')}`);
    
    for (const name of critical) {
      try {
        await this.startDaemon(name);
      } catch (error) {
        console.error(`❌ Failed to start critical daemon ${name}:`, error);
      }
    }
  }

  stopDaemon(name) {
    const daemon = this.daemons.get(name);
    if (daemon) {
      daemon.process.kill();
      this.daemons.delete(name);
      console.log(`🛑 Stopped daemon: ${name}`);
    }
  }

  stopAllDaemons() {
    for (const [name] of this.daemons) {
      this.stopDaemon(name);
    }
  }

  getDaemonStatus() {
    const status = {};
    for (const [name, daemon] of this.daemons) {
      status[name] = {
        running: !daemon.process.killed,
        pid: daemon.process.pid
      };
    }
    return status;
  }
}

module.exports = DaemonManager;