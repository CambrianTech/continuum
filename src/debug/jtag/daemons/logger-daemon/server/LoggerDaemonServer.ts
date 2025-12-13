/**
 * LoggerDaemonServer - DaemonBase wrapper for LoggerDaemon
 *
 * Hybrid architecture:
 * - Extends DaemonBase (registered in JTAGSystem)
 * - Internally spawns LoggerDaemonProcess as child process
 * - Routes IPC messages to child process for log aggregation
 *
 * Benefits:
 * - Standard DaemonBase interface for JTAGSystem
 * - Process isolation for performance
 * - Centralized log file management (160+ log instances)
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { BaseResponsePayload } from '../../../system/core/types/ResponseTypes';
import { createBaseResponse } from '../../../system/core/types/ResponseTypes';
import { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { DaemonBase } from '../../command-daemon/shared/DaemonBase';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';
import { ProcessManager, type ManagedProcess } from '../../../system/core/process/ProcessManager';
import * as path from 'path';

/**
 * LoggerDaemonServer - DaemonBase implementation with child process
 */
export class LoggerDaemonServer extends DaemonBase {
  public readonly subpath = 'logger';
  protected log: ComponentLogger;
  private process?: ManagedProcess;
  private processManager: ProcessManager;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('LoggerDaemon', context, router);

    // Initialize standardized logging
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);

    // Initialize ProcessManager
    this.processManager = new ProcessManager();
  }

  /**
   * Initialize daemon - spawn child process
   */
  protected async initialize(): Promise<void> {
    this.log.info('Initializing LoggerDaemon with child process');

    // Spawn LoggerDaemonProcess as child process using tsx
    // Run TypeScript directly - no compilation needed!
    const scriptPath = path.join(process.cwd(), 'daemons/logger-daemon/process/LoggerDaemonProcess.ts');

    this.process = this.processManager.spawn({
      processId: 'logger-daemon',
      command: 'npx',
      args: ['tsx'],
      scriptPath,
      autoRestart: true,
      maxRestarts: 3,
      restartDelayMs: 1000,
      healthCheckIntervalMs: 5000
    });

    // Start the process
    await this.process.start();

    // REMOVED: IPC path no longer used (Logger.ts now uses direct socket connection)
    // Logger.daemonClient.setProcess(this.process);

    this.log.info('LoggerDaemon child process started with tsx');
  }

  /**
   * Handle incoming messages - route to child process via IPC
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    const sessionId = message.payload.sessionId;

    if (!this.process) {
      return createBaseResponse(false, this.context, sessionId, {
        message: 'LoggerDaemon process not initialized'
      });
    }

    try {
      // Route message to child process via IPC
      // TODO: Implement proper IPC message routing
      // For now, return success
      return createBaseResponse(true, this.context, sessionId, {});
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log.error('Failed to route message to child process:', error);
      return createBaseResponse(false, this.context, sessionId, {
        message: errorMessage
      });
    }
  }

  /**
   * Cleanup daemon - stop child process
   */
  async cleanup(): Promise<void> {
    this.log.info('Shutting down LoggerDaemon');

    if (this.process) {
      await this.process.stop();
      this.process = undefined;
    }

    this.log.info('LoggerDaemon shutdown complete');
  }
}
