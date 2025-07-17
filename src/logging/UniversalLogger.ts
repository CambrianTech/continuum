/**
 * Universal Logger - ONE function to handle all logging with file type separation
 * Uses the exact same pattern as browser logging: server.log, server.error.json, etc.
 * Global logs go to .continuum/logs, session logs go to session/logs directory
 */

import { appendFileSync, mkdirSync, existsSync, statSync } from 'fs';
import * as path from 'path';
import type { ContinuumContext, ContinuumEnvironment } from '../types/shared/core/ContinuumTypes';
import { SessionContext } from '../context/SessionContext';

export class UniversalLogger {
  private static globalLogDir = '.continuum/logs';
  private static initialized = false;
  private static consoleOverridden = false;
  static originalConsole: {
    log: typeof console.log;
    info: typeof console.info;
    warn: typeof console.warn;
    error: typeof console.error;
    debug: typeof console.debug;
  } | null = null;

  static init() {
    if (!this.initialized) {
      try {
        mkdirSync(this.globalLogDir, { recursive: true });
        this.initialized = true;
      } catch (error) {
        console.error('Failed to initialize UniversalLogger:', error);
      }
    }
  }

  /**
   * Override console methods to route to UniversalLogger
   * Similar to browser ConsoleForwarder pattern
   * IMPORTANT: This writes directly to avoid infinite loops with UniversalLogger.log()
   */
  static overrideConsole() {
    if (this.consoleOverridden) {
      return;
    }

    // Store original console methods
    this.originalConsole = {
      log: console.log.bind(console),
      info: console.info.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
      debug: console.debug.bind(console)
    };

    // Override console methods - call original + write JSON directly (no infinite loop)
    console.log = (...args: any[]) => {
      this.originalConsole!.log(...args);
      this.writeConsoleLogDirect('info', args);
    };

    console.info = (...args: any[]) => {
      this.originalConsole!.info(...args);
      this.writeConsoleLogDirect('info', args);
    };

    console.warn = (...args: any[]) => {
      this.originalConsole!.warn(...args);
      this.writeConsoleLogDirect('warn', args);
    };

    console.error = (...args: any[]) => {
      this.originalConsole!.error(...args);
      this.writeConsoleLogDirect('error', args);
    };

    console.debug = (...args: any[]) => {
      this.originalConsole!.debug(...args);
      this.writeConsoleLogDirect('debug', args);
    };

    this.consoleOverridden = true;
  }

  /**
   * Write console log directly to files without calling UniversalLogger.log()
   * to avoid infinite loops. Uses SessionContext for session-aware routing.
   */
  private static writeConsoleLogDirect(level: string, args: any[]) {
    this.init();
    
    const timestamp = new Date().toISOString();
    const message = this.formatConsoleArgs(args);
    const source = 'console';
    const name = 'server';

    // Get current session from SessionContext
    const sessionId = SessionContext.getCurrentSessionSync();
    const contextStr = sessionId ? ` [session:${sessionId}]` : '';

    // Always write to global logs
    this.writeLogFiles(this.globalLogDir, source, `${message}${contextStr}`, level, timestamp, name);

    // If we have a session, also write to session-specific logs
    if (sessionId) {
      try {
        const sessionLogPath = `.continuum/sessions/user/shared/${sessionId}/logs`;
        if (existsSync(sessionLogPath)) {
          this.writeLogFiles(sessionLogPath, source, message, level, timestamp, name);
        }
      } catch (error) {
        // Silently continue if session-specific logging fails
      }
    }
  }

  /**
   * Format console arguments to string
   */
  private static formatConsoleArgs(args: any[]): string {
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

  static log(name: ContinuumEnvironment, source: string, message: string, level: 'info' | 'warn' | 'error' | 'debug' = 'info', context?: ContinuumContext) {
    this.init();
    
    const timestamp = new Date().toISOString();
    
    // Use explicit context first, then fall back to SessionContext
    const sessionId = context?.sessionId ?? SessionContext.getCurrentSessionSync();
    const contextStr = sessionId ? ` [session:${sessionId}]` : '';
    
    // Always log globally to .continuum/logs 
    this.writeLogFiles(this.globalLogDir, source, `${message}${contextStr}`, level, timestamp, name);

    // If context provided, also log to session directory
    if (context && context.sessionPaths?.logs) {
      this.writeLogFiles(context.sessionPaths.logs, source, message, level, timestamp, name);
    }
    // Or if we have a session, log to session directory
    else if (sessionId) {
      try {
        const sessionLogPath = `.continuum/sessions/user/shared/${sessionId}/logs`;
        if (existsSync(sessionLogPath)) {
          this.writeLogFiles(sessionLogPath, source, message, level, timestamp, name);
        }
      } catch (error) {
        // Silently continue if session-specific logging fails
      }
    }
  }

  /**
   * THE ONE FUNCTION - handles any log path with proper type separation
   * Creates: $NAME.log, $NAME.error.json, $NAME.info.json, $NAME.log.json
   */
  private static writeLogFiles(logDir: string, source: string, message: string, level: string, timestamp: string, name: string) {
    try {
      mkdirSync(logDir, { recursive: true });

      // Write human-readable log
      this.writeHumanLog(logDir, source, message, level, timestamp, name);
      
      // Write JSON logs
      this.writeJsonLogs(logDir, source, message, level, timestamp, name);

    } catch (error) {
      console.error(`Failed to write to log path ${logDir}:`, error);
    }
  }

  /**
   * Write human-readable .log file with header
   */
  private static writeHumanLog(logDir: string, source: string, message: string, level: string, timestamp: string, name: string) {
    const humanLogEntry = `UL: [${timestamp}] [${source}] ${level.toUpperCase()}: ${message}\n`;
    const humanLogPath = path.join(logDir, `${name}.log`);
    this.ensureLogFileWithHeader(humanLogPath, name);
    appendFileSync(humanLogPath, humanLogEntry);
  }

  /**
   * Write JSON log files (level-specific and all-levels)
   */
  private static writeJsonLogs(logDir: string, source: string, message: string, level: string, timestamp: string, name: string) {
    const jsonLogEntry = {
      level,
      message,
      timestamp,
      source,
      serverContext: {
        daemon: source,
        processId: process.pid,
        timestamp
      }
    };

    const jsonLine = JSON.stringify(jsonLogEntry) + '\n';

    // Write to level-specific JSON file ($NAME.error.json, $NAME.info.json, etc.) - created on demand
    const levelJsonPath = path.join(logDir, `${name}.${level}.json`);
    appendFileSync(levelJsonPath, jsonLine);

    // Write to all-levels JSON file ($NAME.log.json) - ALL levels - created on demand
    const allJsonPath = path.join(logDir, `${name}.log.json`);
    appendFileSync(allJsonPath, jsonLine);
  }

  /**
   * Ensure log file exists with proper header
   */
  private static ensureLogFileWithHeader(logPath: string, name: string): void {
    try {
      // Check if file exists and has content
      let needsHeader = false;
      try {
        const stats = statSync(logPath);
        needsHeader = stats.size === 0;
      } catch (error) {
        // File doesn't exist, needs header
        needsHeader = true;
      }

      if (needsHeader) {
        const timestamp = new Date().toISOString();
        const header = `# Universal Logger ${name} log
# Created: ${timestamp}
# Type: development
# Owner: shared
#
# Session started at ${timestamp}
#
`;
        appendFileSync(logPath, header);
      }
    } catch (error) {
      console.error(`Failed to write header to ${logPath}:`, error);
    }
  }
}

// Auto-initialize
UniversalLogger.init();