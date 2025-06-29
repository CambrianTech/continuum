/**
 * Version Daemon - Simple test daemon for multi-process architecture
 * Provides version information and system status
 */

import { NodeProcessDaemon } from '../../base/NodeProcessDaemon.js';
import { ProcessMessage, ProcessResult } from '../../interfaces/IProcessCoordinator.js';
import { readFileSync } from 'fs';
import { join } from 'path';

export class VersionDaemon extends NodeProcessDaemon {
  readonly daemonType = 'version';
  readonly capabilities = ['version', 'info'];
  
  private packageVersion: string;
  private startupTime: Date;

  constructor() {
    super();
    this.startupTime = new Date();
    this.packageVersion = this.loadPackageVersion();
  }

  protected async onStart(): Promise<void> {
    this.log('🏷️ Version daemon starting...');
    this.log(`📦 Package version: ${this.packageVersion}`);
    this.log('✅ Version daemon ready');
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Version daemon stopping...');
    this.log('✅ Version daemon stopped');
  }

  protected async onMessage(message: ProcessMessage): Promise<ProcessResult> {
    switch (message.type) {
      case 'version':
        return this.handleVersionRequest();
      
      case 'info':
        return this.handleInfoRequest();
      
      case 'status':
        return this.handleStatusRequest();
      
      default:
        return {
          success: false,
          error: `Unknown message type: ${message.type}`,
          processId: this.processId
        };
    }
  }

  private handleVersionRequest(): ProcessResult {
    return {
      success: true,
      data: {
        version: this.packageVersion,
        daemon: 'VersionDaemon',
        processId: this.processId
      },
      processId: this.processId
    };
  }

  private handleInfoRequest(): ProcessResult {
    const uptime = Date.now() - this.startupTime.getTime();
    
    return {
      success: true,
      data: {
        version: this.packageVersion,
        daemon: this.daemonType,
        processId: this.processId,
        uptime,
        startupTime: this.startupTime.toISOString(),
        capabilities: this.capabilities,
        pid: process.pid,
        nodeVersion: process.version,
        platform: process.platform,
        architecture: process.arch
      },
      processId: this.processId
    };
  }

  private handleStatusRequest(): ProcessResult {
    const health = this.getHealth();
    
    return {
      success: true,
      data: {
        ...health,
        version: this.packageVersion,
        ready: true
      },
      processId: this.processId
    };
  }

  private loadPackageVersion(): string {
    try {
      const packagePath = join(process.cwd(), 'package.json');
      const packageData = JSON.parse(readFileSync(packagePath, 'utf8'));
      return packageData.version || '0.0.0';
    } catch (error) {
      this.log(`⚠️ Could not load package version: ${error.message}`, 'warn');
      return 'unknown';
    }
  }
}

// Run daemon when executed directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const daemon = new VersionDaemon();
  
  // Graceful shutdown handlers
  process.on('SIGINT', async () => {
    await daemon.gracefulShutdown();
  });
  
  process.on('SIGTERM', async () => {
    await daemon.gracefulShutdown();
  });
  
  // Start the daemon
  daemon.start().catch(error => {
    console.error('❌ Version daemon failed to start:', error);
    process.exit(1);
  });
}