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
import * as path from 'path';

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
   * Write multiple log entries to session files
   */
  private async writeBatchedEntries(entries: LogEntry[]): Promise<void> {
    if (entries.length === 0) return;

    const sessionId = entries[0].context.sessionId;
    const sessionDir = this.getSessionLogDirectory(sessionId);
    
    // Group by log level for separate files
    const levelGroups = new Map<LogLevel, LogEntry[]>();
    for (const entry of entries) {
      if (!levelGroups.has(entry.level)) {
        levelGroups.set(entry.level, []);
      }
      levelGroups.get(entry.level)!.push(entry);
    }

    // Write to combined log and level-specific files
    const allEntriesText = entries.map(entry => this.formatLogEntry(entry)).join('\n') + '\n';
    await this.writeToFile(path.join(sessionDir, 'server.log'), allEntriesText);

    // Write to level-specific JSON files
    for (const [level, levelEntries] of levelGroups) {
      const jsonData = levelEntries.map(entry => this.formatLogEntryJSON(entry)).join('\n') + '\n';
      await this.writeToFile(path.join(sessionDir, `server.${level}.json`), jsonData);
    }
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

  /**
   * Get session log directory
   */
  private getSessionLogDirectory(sessionId: string): string {
    // This should match your existing session directory structure
    return path.join(process.cwd(), '.continuum', 'sessions', 'user', 'shared', sessionId, 'logs');
  }

  /**
   * Write to file with proper file handle management
   */
  private async writeToFile(filePath: string, content: string): Promise<void> {
    const dir = path.dirname(filePath);
    await fs.mkdir(dir, { recursive: true });
    await fs.appendFile(filePath, content, 'utf8');
  }

  /**
   * Format log entry for human-readable log file
   */
  private formatLogEntry(entry: LogEntry): string {
    return `UL: [${entry.timestamp}] [${entry.source}] ${entry.level.toUpperCase()}: ${entry.message} [session:${entry.context.sessionId}]`;
  }

  /**
   * Format log entry for JSON log file
   */
  private formatLogEntryJSON(entry: LogEntry): string {
    return JSON.stringify({
      level: entry.level,
      message: entry.message,
      timestamp: entry.timestamp,
      source: entry.source,
      context: entry.context,
      data: entry.data
    });
  }

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
   * Log console message through daemon queue
   */
  private logConsoleMessage(level: LogLevel, args: any[]): void {
    try {
      const message = this.formatConsoleArgs(args);
      const logEntry: LogEntry = {
        level,
        message,
        timestamp: Date.now(),
        sessionId: this.context?.sessionId || 'server',
        source: 'console',
        context: this.context || { sessionId: 'server' as any, environment: 'server' }
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