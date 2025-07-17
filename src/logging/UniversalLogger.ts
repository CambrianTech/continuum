/**
 * Universal Logger - ONE function to handle all logging with file type separation
 * Uses the exact same pattern as browser logging: server.log, server.error.json, etc.
 * Global logs go to .continuum/logs, session logs go to session/logs directory
 */

import { appendFileSync, mkdirSync, statSync } from 'fs';
import * as path from 'path';
import type { ContinuumContext, ContinuumEnvironment } from '../types/shared/core/ContinuumTypes';

export class UniversalLogger {
  private static globalLogDir = '.continuum/logs';
  private static initialized = false;

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

  static log(name: ContinuumEnvironment, source: string, message: string, level: 'info' | 'warn' | 'error' | 'debug' = 'info', context?: ContinuumContext) {
    this.init();
    
    const timestamp = new Date().toISOString();
    const contextStr = context ? ` [session:${context.sessionId}]` : '';
    
    // Always log globally to .continuum/logs 
    this.writeLogFiles(this.globalLogDir, source, `${message}${contextStr}`, level, timestamp, name);

    // If context provided, also log to session directory
    if (context && context.sessionPaths?.logs) {
      this.writeLogFiles(context.sessionPaths.logs, source, message, level, timestamp, name);
    }
    
    // Special handling for session-specific logging when context.sessionId is provided
    if (context?.sessionId && name === 'browser') {
      try {
        const path = require('path');
        const fs = require('fs');
        const sessionLogPath = path.join('.continuum/sessions/user/shared', context.sessionId, 'logs');
        
        // Ensure directory exists
        if (!fs.existsSync(sessionLogPath)) {
          fs.mkdirSync(sessionLogPath, { recursive: true });
        }
        
        this.writeLogFiles(sessionLogPath, source, message, level, timestamp, name);
      } catch (error) {
        console.error('Failed to write to session-specific browser.log:', error);
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
    const humanLogEntry = `[${timestamp}] [${source}] ${level.toUpperCase()}: ${message}\n`;
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