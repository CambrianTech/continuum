/**
 * Universal Logger - ONE function to handle all logging with file type separation
 * Uses the exact same pattern as browser logging: server.log, server.error.json, etc.
 * Global logs go to .continuum/logs, session logs go to session/logs directory
 */

import { appendFileSync, mkdirSync, existsSync, statSync, readFileSync, writeFileSync, readdirSync } from 'fs';
import * as path from 'path';
import type { ContinuumContext, ContinuumEnvironment } from '../../types/shared/core/ContinuumTypes';
// REMOVED: SessionContext import - no longer needed since LoggerDaemon handles console override

export class UniversalLogger {
  private static globalLogDir = '.continuum/logs';
  private static initialized = false;
  // REMOVED: consoleOverridden and originalConsole - LoggerDaemon handles console override

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
   * CONSOLE OVERRIDE REMOVED: LoggerDaemon is now "The Console Daemon"
   * All console override functionality has been moved to LoggerDaemon with semaphore protection
   * This method is deprecated and will be removed in a future version
   */
  static async overrideConsole() {
    console.log('🔧 UniversalLogger: Console override removed - LoggerDaemon is now "The Console Daemon"');
    console.log('🔧 UniversalLogger: This method is deprecated and will be removed');
  }

  /**
   * REMOVED: Console override functionality moved to LoggerDaemon
   * This method is no longer needed since LoggerDaemon handles console override
   */

  /**
   * REMOVED: writeConsoleLogDirect - No longer needed since LoggerDaemon handles console override
   */

  /**
   * REMOVED: getContextFromStack - No longer needed since LoggerDaemon handles console override
   */

  /**
   * REMOVED: getSourceFromStack - No longer needed since LoggerDaemon handles console override
   */

  /**
   * REMOVED: formatConsoleArgs - No longer needed since LoggerDaemon handles console override
   */

  static async log(name: ContinuumEnvironment, source: string, message: string, level: 'info' | 'warn' | 'error' | 'debug' = 'info', context: ContinuumContext) {
    try {
      // Use async logger with stack-based context
      const { loggerClient } = await import('./server/LoggerClient');
      await loggerClient.log(context, level, message, source);
    } catch (error) {
      // Fallback to original sync logging if async fails
      this.logSync(name, source, message, level, context);
    }
  }

  /**
   * Synchronous logging fallback
   */
  static logSync(name: ContinuumEnvironment, source: string, message: string, level: 'info' | 'warn' | 'error' | 'debug' = 'info', context: ContinuumContext) {
    this.init();
    
    const timestamp = new Date().toISOString();
    
    // Use explicit context (now required)
    const sessionId = context.sessionId;
    const contextStr = sessionId ? ` [session:${sessionId}]` : '';
    
    // Always log globally to .continuum/logs 
    this.writeLogFiles(this.globalLogDir, source, message, level, timestamp, name, contextStr);

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
   * REMOVED: Console override functionality moved to LoggerDaemon
   * This method is deprecated and will be removed in a future version
   */
  static restoreConsole() {
    console.log('🔧 UniversalLogger: Console restore removed - LoggerDaemon handles console override');
    console.log('🔧 UniversalLogger: This method is deprecated and will be removed');
  }

  /**
   * Async logging method - bypasses console overrides to avoid infinite loops
   */
  public static logAsync(message: string, level: string = 'info'): void {
    const timestamp = new Date().toISOString();
    const source = 'WebSocketDaemon';
    
    // Write to global logs
    this.writeLogFiles(this.globalLogDir, source, message, level, timestamp, 'browser');
    
    // Try to write to session logs if we can determine session context
    try {
      const sessionDirs = readdirSync('.continuum/sessions/user/shared');
      if (sessionDirs.length > 0) {
        // Use the most recent session directory
        const sessionLogPath = `.continuum/sessions/user/shared/${sessionDirs[0]}/logs`;
        if (existsSync(sessionLogPath)) {
          this.writeLogFiles(sessionLogPath, source, message, level, timestamp, 'browser');
        }
      }
    } catch (error) {
      // Silently continue if session-specific logging fails
    }
  }

  /**
   * THE ONE FUNCTION - handles any log path with proper type separation
   * Creates: $NAME.log, $NAME.error.json, $NAME.info.json, $NAME.log.json
   */
  public static writeLogFiles(logDir: string, source: string, message: string, level: string, timestamp: string, name: string, contextStr: string = '') {
    try {
      mkdirSync(logDir, { recursive: true });

      // Write human-readable log (includes context for global logs)
      this.writeHumanLog(logDir, source, message + contextStr, level, timestamp, name);
      
      // Write JSON logs (uses original message without context string)
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
   * Write JSON log files (level-specific and all-levels) as proper JSON arrays
   */
  private static writeJsonLogs(logDir: string, source: string, message: string, level: string, timestamp: string, name: string) {
    // Try to parse message as JSON - if it's a JSON string, use the parsed object
    let messageContent: any = message;
    try {
      const parsed = JSON.parse(message);
      messageContent = parsed;
    } catch (error) {
      // Not JSON, keep as string
      messageContent = message;
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

    // Write to level-specific JSON file ($NAME.error.json, $NAME.info.json, etc.) - created on demand
    const levelJsonPath = path.join(logDir, `${name}.${level}.json`);
    this.appendToJsonArray(levelJsonPath, jsonLogEntry, name, level);

    // Write to all-levels JSON file ($NAME.log.json) - ALL levels - created on demand
    const allJsonPath = path.join(logDir, `${name}.log.json`);
    this.appendToJsonArray(allJsonPath, jsonLogEntry, name, 'log');
  }

  /**
   * Load JSON template for log files
   */
  private static loadLogTemplate(): any {
    try {
      const templatePath = path.join(__dirname, 'templates', 'log-template.json');
      const templateContent = readFileSync(templatePath, 'utf8');
      return JSON.parse(templateContent);
    } catch (error) {
      console.error('Failed to load log template, using fallback:', error);
      // Fallback template if file can't be loaded
      return {
        meta: {
          version: '1.0.0'
        },
        entries: []
      };
    }
  }

  /**
   * Append entry to JSON array file with proper structure
   */
  private static appendToJsonArray(filePath: string, entry: any, name: string, logType: string) {
    try {
      let fileContent: any;
      
      // Check if file exists and has content
      if (existsSync(filePath)) {
        try {
          const content = readFileSync(filePath, 'utf8');
          if (content.trim()) {
            fileContent = JSON.parse(content);
          }
        } catch (error) {
          // File exists but is corrupted, start fresh
          fileContent = null;
        }
      }

      // If no valid content, create new structure from template
      if (!fileContent) {
        const template = this.loadLogTemplate();
        const createdTime = new Date().toISOString();
        
        fileContent = {
          ...template,
          meta: {
            ...template.meta,
            created: createdTime,
            type: 'development',
            owner: 'shared',
            logType: logType,
            name: name
          }
        };
      }

      // Add new entry
      fileContent.entries.push(entry);

      // Write back to file
      writeFileSync(filePath, JSON.stringify(fileContent, null, 2));

    } catch (error) {
      console.error(`Failed to append to JSON array ${filePath}:`, error);
      
      // Fallback: write as NDJSON if JSON array approach fails
      const jsonLine = JSON.stringify(entry) + '\n';
      try {
        appendFileSync(filePath, jsonLine);
      } catch (fallbackError) {
        console.error(`Fallback NDJSON write also failed:`, fallbackError);
      }
    }
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