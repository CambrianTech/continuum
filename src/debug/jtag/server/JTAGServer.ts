/**
 * Emergency JTAG - Server Implementation
 * 
 * Server-specific Emergency JTAG implementation.
 * Provides server-side logging with file system persistence.
 */

import { JTAGBase } from '../shared/JTAGBase';
import type { JTAGConfig } from '../shared/JTAGTypes';

export class EmergencyJTAGServer extends JTAGBase {
  /**
   * Initialize Emergency JTAG for server context
   */
  static initializeServer(overrides?: Partial<JTAGConfig>): void {
    const serverConfig = {
      context: 'server' as const,
      logDirectory: '.continuum/logs',
      enableRemoteLogging: false,
      enableConsoleOutput: true,
      ...overrides
    };
    
    super.initialize(serverConfig);
  }

  /**
   * Server-specific diagnostic info
   */
  static getServerDiagnostics() {
    const process_info = typeof process !== 'undefined' ? {
      pid: process.pid,
      version: process.version,
      platform: process.platform,
      arch: process.arch,
      uptime: process.uptime(),
      memoryUsage: process.memoryUsage()
    } : null;

    return {
      nodeVersion: process_info?.version || 'Unknown',
      platform: process_info?.platform || 'Unknown',
      pid: process_info?.pid || 0,
      uptime: process_info?.uptime || 0,
      memory: process_info?.memoryUsage || null,
      timestamp: new Date().toISOString()
    };
  }

  /**
   * Log server-specific information
   */
  static logServerContext(component: string, message: string, includeContext = true): void {
    const data = includeContext ? this.getServerDiagnostics() : undefined;
    super.log(component, `[SERVER] ${message}`, data);
  }

  /**
   * Force flush logs to disk (server-only operation)
   */
  static async flushToDisk(): Promise<void> {
    const logs = this.getLogs();
    if (logs.length > 0) {
      for (const entry of logs) {
        // Force write each log entry
        this.probe('JTAG_SERVER', 'FLUSHING_LOG', entry);
      }
    }
  }
}