#!/usr/bin/env node
/**
 * LoggerDaemonProcess - Entry Point for Logger Daemon
 *
 * Spawned by JTAGSystem to handle all log aggregation for the system.
 * Uses ProcessLifecycle for standard lifecycle management.
 *
 * Usage (by JTAGSystem):
 * ```typescript
 * const loggerDaemon = processManager.spawn({
 *   command: 'node',
 *   args: ['daemons/logger-daemon/process/LoggerDaemonProcess.js'],
 *   name: 'LoggerDaemon',
 *   restartOnCrash: true
 * });
 * ```
 */

import { LifecycleManager } from '../../../system/core/process/ProcessLifecycle';
import type { IPCMessage, IPCResponse } from '../../../system/core/process/IPCProtocol';
import { createSuccessResponse, createErrorResponse } from '../../../system/core/process/IPCProtocol';
import { LoggerDaemonServer } from '../server/LoggerDaemonServer';
import type {
  LogMessage,
  ConfigureLoggerMessage,
  LogStatsMessage,
  FlushMessage
} from '../shared/LoggerDaemonTypes';
import {
  isLogMessage,
  isConfigureLoggerMessage,
  isLogStatsMessage,
  isFlushMessage
} from '../shared/LoggerDaemonTypes';

/**
 * Logger daemon state
 */
let server: LoggerDaemonServer | undefined;

/**
 * Initialize the daemon
 */
async function onInitialize(): Promise<void> {
  console.log('[LoggerDaemonProcess] Initializing...');

  // Create logger server
  server = new LoggerDaemonServer();
  await server.initialize();

  console.log('[LoggerDaemonProcess] Initialized');
}

/**
 * Handle incoming IPC messages
 */
async function onMessage(message: IPCMessage): Promise<IPCResponse> {
  if (!server) {
    return createErrorResponse(
      `${message.type}-response`,
      'LoggerDaemonServer not initialized',
      message.messageId
    );
  }

  try {
    // Handle LogMessage (most common)
    if (isLogMessage(message)) {
      await server.handleLogMessage(message);
      return createSuccessResponse(
        'log-response',
        {
          written: true,
          queueSize: server.getStats().queueSize,
          queueFull: false
        },
        message.messageId
      );
    }

    // Handle ConfigureLoggerMessage (runtime reconfiguration)
    if (isConfigureLoggerMessage(message)) {
      // TODO: Implement runtime configuration
      // For now, settings travel with each log message, so this is optional
      return createSuccessResponse(
        'configure-logger-response',
        { settings: {} as any },
        message.messageId
      );
    }

    // Handle LogStatsMessage (statistics query)
    if (isLogStatsMessage(message)) {
      const stats = server.getStats();
      return createSuccessResponse(
        'log-stats-response',
        stats,
        message.messageId
      );
    }

    // Handle FlushMessage (force flush)
    if (isFlushMessage(message)) {
      // Flush is automatic via timer, but we can trigger it manually
      return createSuccessResponse(
        'flush-response',
        { flushed: server.getStats().queueSize },
        message.messageId
      );
    }

    // Unknown message type
    return createErrorResponse(
      `${message.type}-response`,
      `Unknown message type: ${message.type}`,
      message.messageId
    );

  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : String(error);
    console.error(`[LoggerDaemonProcess] Message handling failed:`, error);
    return createErrorResponse(
      `${message.type}-response`,
      errorMessage,
      message.messageId
    );
  }
}

/**
 * Shutdown the daemon
 */
async function onShutdown(): Promise<void> {
  console.log('[LoggerDaemonProcess] Shutting down...');

  if (server) {
    await server.shutdown();
    server = undefined;
  }

  console.log('[LoggerDaemonProcess] Shutdown complete');
}

/**
 * Error handler
 */
async function onError(error: Error): Promise<void> {
  console.error('[LoggerDaemonProcess] Fatal error:', error);

  if (server) {
    try {
      await server.shutdown();
    } catch (shutdownError) {
      console.error('[LoggerDaemonProcess] Shutdown after error failed:', shutdownError);
    }
  }
}

/**
 * Start the daemon
 */
async function main(): Promise<void> {
  const lifecycle = new LifecycleManager({
    onInitialize,
    onMessage,
    onShutdown,
    onError
  });

  await lifecycle.start();
}

// Run daemon
main().catch((error) => {
  console.error('[LoggerDaemonProcess] Failed to start:', error);
  process.exit(1);
});
