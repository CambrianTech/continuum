/**
 * Logger Daemon - Process-based async logging with queue processing
 * First implementation of ProcessBasedDaemon architecture
 */

import { ProcessBasedDaemon } from '../../base/ProcessBasedDaemon';
import { DaemonResponse } from '../../base/DaemonProtocol';
import { DaemonType } from '../../base/DaemonTypes';
import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';
import { 
  LoggerDaemonMessage, 
  LoggerMessage, 
  LogEntry, 
  FlushRequest, 
  RotateRequest,
  ConfigureRequest,
  LogLevel
} from '../shared/LoggerMessageTypes';
import { ConsoleOverrideSemaphore } from '../shared/ConsoleOverrideSemaphore';
import * as fs from 'fs/promises';

export class LoggerDaemon extends ProcessBasedDaemon<LoggerMessage> {
  readonly name = 'logger';
  readonly version = '1.0.0';
  readonly daemonType: DaemonType = 'logger' as DaemonType;

  private openFileHandles = new Map<string, fs.FileHandle>();
  private logBuffers = new Map<string, LogEntry[]>();
  protected config = {
    batchSize: 100,
    flushInterval: 5000, // 5 seconds
    enableBatching: true,
    logLevel: 'info' as LogLevel
  };
  
  // Store original console methods to avoid infinite loops
  private originalConsole = {
    log: console.log,
    warn: console.warn,
    error: console.error,
    info: console.info,
    debug: console.debug
  };

  // private consoleOverridden = false; // TODO: Re-enable with console override

  constructor(context?: ContinuumContext) {
    super(context, {
      queueSize: 10000,
      batchSize: 100,
      processTimeoutMs: 30000,
      resourceLimits: {
        memory: '64MB',
        cpu: '10%'
      }
    });

    // Start periodic flush timer
    this.startPeriodicFlush();
    
    // LoggerDaemon is THE Console Daemon - it owns all console override logic
    // Console override will be enabled separately after daemon system is stable
  }

  /**
   * Process a single logger message
   */
  protected async processMessage(message: LoggerDaemonMessage): Promise<DaemonResponse> {
    try {
      const { type, payload } = message.data;

      switch (type) {
        case 'log':
          await this.handleLogMessage(payload as LogEntry);
          break;
        case 'flush':
          await this.handleFlushMessage(payload as FlushRequest);
          break;
        case 'rotate':
          await this.handleRotateMessage(payload as RotateRequest);
          break;
        case 'configure':
          await this.handleConfigureMessage(payload as ConfigureRequest);
          break;
        default:
          throw new Error(`Unknown message type: ${type}`);
      }

      return {
        success: true,
        messageId: message.id,
        processingTime: Date.now() - message.timestamp.getTime()
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        messageId: message.id
      };
    }
  }

  /**
   * Process batch of log messages for efficiency
   */
  protected async processBatch(messages: LoggerDaemonMessage[]): Promise<DaemonResponse[]> {
    const logMessages = messages.filter(msg => msg.data.type === 'log');
    const otherMessages = messages.filter(msg => msg.data.type !== 'log');

    const responses: DaemonResponse[] = [];

    // Process log messages in batch
    if (logMessages.length > 0) {
      try {
        await this.processBatchedLogs(logMessages);
        responses.push(...logMessages.map(msg => ({
          success: true,
          messageId: msg.id,
          processingTime: Date.now() - msg.timestamp.getTime()
        })));
      } catch (error) {
        responses.push(...logMessages.map(msg => ({
          success: false,
          error: error instanceof Error ? error.message : String(error),
          messageId: msg.id
        })));
      }
    }

    // Process other messages individually
    for (const message of otherMessages) {
      responses.push(await this.processMessage(message));
    }

    return responses;
  }

  /**
   * Handle log message - add to buffer or write immediately
   */
  private async handleLogMessage(logEntry: LogEntry): Promise<void> {
    if (this.config.enableBatching) {
      await this.addToBuffer(logEntry);
    } else {
      await this.writeLogEntry(logEntry);
    }
  }

  /**
   * Process batched log messages efficiently
   */
  private async processBatchedLogs(messages: LoggerDaemonMessage[]): Promise<void> {
    const logEntries = messages.map(msg => msg.data.payload as LogEntry);
    
    // Group by session for efficient file operations
    const sessionGroups = new Map<string, LogEntry[]>();
    for (const entry of logEntries) {
      const sessionId = entry.context.sessionId;
      if (!sessionGroups.has(sessionId)) {
        sessionGroups.set(sessionId, []);
      }
      sessionGroups.get(sessionId)!.push(entry);
    }

    // Write each session's logs in batch
    for (const [, entries] of sessionGroups) {
      await this.writeBatchedEntries(entries);
    }
  }

  /**
   * THE ONE METHOD - Write log entries to both global and session directories
   */
  private async writeBatchedEntries(entries: LogEntry[]): Promise<void> {
    if (entries.length === 0) return;

    for (const entry of entries) {
      await this.writeOneLogEntry(entry);
    }
  }

  /**
   * THE ONE ENTRY POINT - Write single log entry to all required locations
   */
  private async writeOneLogEntry(entry: LogEntry): Promise<void> {
    const timestamp = new Date(entry.timestamp).toISOString();
    const sessionId = entry.context.sessionId || 'system';
    
    // Write to global logs (with session context)
    await this.writeToLocation('.continuum/logs', entry, timestamp, true);
    
    // Write to session logs (without session context) if not system
    if (sessionId !== 'system') {
      const sessionPath = `.continuum/sessions/user/shared/${sessionId}/logs`;
      await this.writeToLocation(sessionPath, entry, timestamp, false);
    }
  }

  /**
   * THE ONE METHOD - Write to one location using EXACTLY the same logic as browser (sync version)
   */
  private async writeToLocation(logDir: string, entry: LogEntry, timestamp: string, includeSessionContext: boolean): Promise<void> {
    const syncFs = await import('fs');
    const path = await import('path');
    
    const message = typeof entry.message === 'string' ? entry.message : JSON.stringify(entry.message);
    const contextStr = includeSessionContext && entry.context.sessionId ? ` [session:${entry.context.sessionId}]` : '';
    
    try {
      syncFs.mkdirSync(logDir, { recursive: true });

      // Write human-readable log (EXACT SAME as browser)
      const humanLogEntry = `UL: [${timestamp}] [${entry.source}] ${entry.level.toUpperCase()}: ${message}${contextStr}\n`;
      const humanLogPath = path.join(logDir, 'server.log');
      this.ensureLogFileWithHeaderSync(humanLogPath, 'server', syncFs);
      syncFs.appendFileSync(humanLogPath, humanLogEntry);

      // Write JSON logs (EXACT SAME as browser)
      this.writeJsonLogsSync(logDir, entry.source, message, entry.level, timestamp, 'server', syncFs, path);

    } catch (error) {
      // Don't use console.error - would cause circular dependency
      // Just fail silently or use original console
      this.originalConsole.error(`Failed to write to log path ${logDir}:`, error);
    }
  }

  /**
   * Ensure log file exists with proper header (sync version)
   */
  private ensureLogFileWithHeaderSync(logPath: string, name: string, syncFs: any): void {
    try {
      syncFs.statSync(logPath);
    } catch {
      // File doesn't exist, create with header
      const header = `# Universal Logger ${name} log\n# Created: ${new Date().toISOString()}\n# Type: development\n# Owner: shared\n#\n# Session started at ${new Date().toISOString()}\n#\n`;
      syncFs.writeFileSync(logPath, header);
    }
  }

  /**
   * Write JSON logs (EXACT SAME template format as browser, sync version)
   */
  private writeJsonLogsSync(logDir: string, source: string, message: string, level: string, timestamp: string, name: string, syncFs: any, path: any): void {
    // Parse message as JSON if possible
    let messageContent: any = message;
    try {
      const parsed = JSON.parse(message);
      messageContent = parsed;
    } catch {
      // Not JSON, keep as string
    }

    const jsonLogEntry = {
      level,
      message: messageContent,
      timestamp,
      source,
      serverContext: {
        daemon: source,
        processId: process.pid,
        timestamp
      }
    };

    // Write level-specific JSON file with template
    const jsonFile = path.join(logDir, `${name}.${level}.json`);
    this.appendToJsonTemplateSync(jsonFile, jsonLogEntry, level, name, syncFs);
  }

  /**
   * Append to JSON template file (EXACT SAME format as browser, sync version)
   */
  private appendToJsonTemplateSync(filePath: string, entry: any, level: string, name: string, syncFs: any): void {
    let template: any;
    
    try {
      const existing = syncFs.readFileSync(filePath, 'utf8');
      template = JSON.parse(existing);
    } catch {
      // File doesn't exist, create template
      template = {
        meta: {
          version: '1.0.0',
          created: new Date().toISOString(),
          type: 'development',
          owner: 'shared',
          logType: level,
          name: name
        },
        entries: []
      };
    }

    template.entries.push(entry);
    syncFs.writeFileSync(filePath, JSON.stringify(template, null, 2));
  }

  /**
   * Add log entry to buffer
   */
  private async addToBuffer(logEntry: LogEntry): Promise<void> {
    const sessionId = logEntry.context.sessionId;
    if (!this.logBuffers.has(sessionId)) {
      this.logBuffers.set(sessionId, []);
    }
    
    this.logBuffers.get(sessionId)!.push(logEntry);
    
    // Flush if buffer is full
    if (this.logBuffers.get(sessionId)!.length >= this.config.batchSize) {
      await this.flushBuffer(sessionId);
    }
  }

  /**
   * Write single log entry immediately
   */
  private async writeLogEntry(logEntry: LogEntry): Promise<void> {
    await this.writeBatchedEntries([logEntry]);
  }

  /**
   * Handle flush message
   */
  private async handleFlushMessage(flushRequest: FlushRequest): Promise<void> {
    if (flushRequest.sessionId) {
      await this.flushBuffer(flushRequest.sessionId);
    } else {
      await this.flushAllBuffers();
    }
  }

  /**
   * Handle rotate message
   */
  private async handleRotateMessage(rotateRequest: RotateRequest): Promise<void> {
    // Implementation for log rotation
    // This is a placeholder - would implement actual rotation logic
    this.originalConsole.log('Log rotation requested:', rotateRequest);
  }

  /**
   * Handle configure message
   */
  private async handleConfigureMessage(configureRequest: ConfigureRequest): Promise<void> {
    this.config = { ...this.config, ...configureRequest };
  }

  /**
   * Flush buffer for specific session
   */
  private async flushBuffer(sessionId: string): Promise<void> {
    const buffer = this.logBuffers.get(sessionId);
    if (!buffer || buffer.length === 0) return;

    await this.writeBatchedEntries(buffer);
    this.logBuffers.set(sessionId, []);
  }

  /**
   * Flush all buffers
   */
  private async flushAllBuffers(): Promise<void> {
    for (const sessionId of this.logBuffers.keys()) {
      await this.flushBuffer(sessionId);
    }
  }

  /**
   * Start periodic flush timer
   */
  private startPeriodicFlush(): void {
    setInterval(async () => {
      try {
        await this.flushAllBuffers();
      } catch (error) {
        this.originalConsole.error('Periodic flush error:', error);
      }
    }, this.config.flushInterval);
  }

  // Removed unused methods - now using UniversalLogger.writeLogFiles for consistency

  /**
   * Clean shutdown - flush all buffers and close file handles
   */
  async stop(): Promise<void> {
    // Release console override semaphore
    this.disableConsoleOverride();
    
    await this.flushAllBuffers();
    
    // Close all file handles
    for (const [filePath, handle] of this.openFileHandles) {
      try {
        await handle.close();
      } catch (error) {
        this.originalConsole.error(`Error closing file handle for ${filePath}:`, error);
      }
    }
    this.openFileHandles.clear();
    
    await super.stop();
  }

  /**
   * Enable console override using the global semaphore
   * LoggerDaemon is THE Console Daemon - it owns all console override logic
   * Call this after daemon system is stable
   */
  public enableConsoleOverride(): void {
    try {
      // Acquire the global console override semaphore
      ConsoleOverrideSemaphore.acquire('LoggerDaemon');
      
      // Get the original console methods from the semaphore
      const originalConsole = ConsoleOverrideSemaphore.getOriginalConsole();
      if (!originalConsole) {
        throw new Error('Failed to get original console methods from semaphore');
      }

      // Override console methods to route through LoggerDaemon
      console.log = (...args: any[]) => {
        originalConsole.log(...args);
        this.logConsoleMessage('info', args);
      };

      console.info = (...args: any[]) => {
        originalConsole.info(...args);
        this.logConsoleMessage('info', args);
      };

      console.warn = (...args: any[]) => {
        originalConsole.warn(...args);
        this.logConsoleMessage('warn', args);
      };

      console.error = (...args: any[]) => {
        originalConsole.error(...args);
        this.logConsoleMessage('error', args);
      };

      console.debug = (...args: any[]) => {
        originalConsole.debug(...args);
        this.logConsoleMessage('debug', args);
      };

      originalConsole.log('✅ LoggerDaemon: Console override enabled with semaphore protection');
    } catch (error) {
      // If semaphore acquisition fails, log the error but don't crash the daemon
      console.error('❌ LoggerDaemon: Failed to acquire console override semaphore:', error);
    }
  }

  /**
   * Disable console override by releasing the semaphore
   */
  private disableConsoleOverride(): void {
    try {
      if (ConsoleOverrideSemaphore.isActive() && ConsoleOverrideSemaphore.getCurrentSource() === 'LoggerDaemon') {
        ConsoleOverrideSemaphore.release('LoggerDaemon');
      }
    } catch (error) {
      console.error('❌ LoggerDaemon: Failed to release console override semaphore:', error);
    }
  }

  /**
   * Public async log method - THE ONE METHOD for all logging
   * Replaces all UniversalLogger calls
   */
  public static async log(
    message: string, 
    level: LogLevel = 'info',
    source: string = 'unknown',
    sessionId?: string,
    context?: ContinuumContext
  ): Promise<void> {
    try {
      const logEntry: LogEntry = {
        level,
        message,
        timestamp: Date.now(),
        sessionId: sessionId || context?.sessionId || 'system',
        source,
        context: context || { sessionId: (sessionId || 'system') as any, environment: 'server' }
      };

      // Get the singleton instance and queue the message
      const instance = LoggerDaemon.getInstance();
      if (instance) {
        instance.enqueueMessage({
          id: instance.generateMessageId(),
          from: 'logger',
          to: 'logger',
          type: 'log',
          data: {
            type: 'log',
            payload: logEntry
          },
          timestamp: new Date()
        });
      } else {
        // Fallback to console if daemon not available
        console.log(`[${new Date().toISOString()}] [${source}] ${level.toUpperCase()}: ${message}`);
      }
    } catch (error) {
      // Fallback to console to avoid infinite loops
      console.error('❌ LoggerDaemon.log error:', error);
    }
  }

  /**
   * Log console message through daemon queue (for console override)
   */
  private logConsoleMessage(level: LogLevel, args: any[]): void {
    try {
      const message = this.formatConsoleArgs(args);
      
      // Try to extract session context from call stack
      const sessionContext = this.extractSessionFromCallStack();
      
      const logEntry: LogEntry = {
        level,
        message,
        timestamp: Date.now(),
        sessionId: sessionContext?.sessionId || 'system',
        source: 'console',
        context: sessionContext || { sessionId: 'system' as any, environment: 'server' }
      };

      // Queue the message for async processing
      this.enqueueMessage({
        id: this.generateMessageId(),
        from: 'logger',
        to: 'logger',
        type: 'log',
        data: {
          type: 'log',
          payload: logEntry
        },
        timestamp: new Date()
      });
    } catch (error) {
      // Fallback to original console to avoid infinite loops
      const originalConsole = ConsoleOverrideSemaphore.getOriginalConsole();
      if (originalConsole) {
        originalConsole.error('❌ LoggerDaemon console override error:', error);
      }
    }
  }

  /**
   * Extract session context from call stack
   */
  private extractSessionFromCallStack(): ContinuumContext | null {
    try {
      const stack = new Error().stack;
      if (!stack) return null;

      // Look for session indicators in the stack
      const sessionMatch = stack.match(/session[:-]([a-f0-9-]{36})/i);
      if (sessionMatch) {
        return {
          sessionId: sessionMatch[1] as any,
          environment: 'server'
        };
      }
      
      return null;
    } catch (error) {
      return null;
    }
  }

  /**
   * Singleton instance access
   */
  private static instance: LoggerDaemon | null = null;
  
  public static getInstance(): LoggerDaemon | null {
    return LoggerDaemon.instance;
  }
  
  protected async onStart(): Promise<void> {
    LoggerDaemon.instance = this;
    await super.onStart();
  }
  
  protected async onStop(): Promise<void> {
    LoggerDaemon.instance = null;
    await super.onStop();
  }

  /**
   * Format console arguments to string
   */
  private formatConsoleArgs(args: any[]): string {
    return args.map(arg => {
      if (typeof arg === 'string') {
        return arg;
      } else if (typeof arg === 'object' && arg !== null) {
        try {
          return JSON.stringify(arg);
        } catch (error) {
          return String(arg);
        }
      } else {
        return String(arg);
      }
    }).join(' ');
  }
}