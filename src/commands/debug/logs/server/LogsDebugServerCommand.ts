/**
 * Logs Debug Server Command
 * 
 * Server implementation - handles filesystem access to log files
 * Uses Node.js fs module and proper file system operations
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { LogsDebugParams, LogsDebugResult, LogEntry, LogFileInfo } from '../shared/LogsDebugTypes';
import { createLogsDebugResult } from '../shared/LogsDebugTypes';
import { promises as fs, constants as fsConstants } from 'fs';
import * as path from 'path';

export class LogsDebugServerCommand extends CommandBase<LogsDebugParams, LogsDebugResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('logs', context, subpath, commander);
  }
  
  async execute(params: LogsDebugParams): Promise<LogsDebugResult> {
    try {
      // SIMPLE & FAST: Just grep npm-start.log (the only log that matters)
      const npmStartLog = path.join(process.cwd(), '.continuum', 'jtag', 'system', 'logs', 'npm-start.log');

      // Check if log exists
      const logExists = await fs.access(npmStartLog).then(() => true).catch(() => false);
      if (!logExists) {
        return createLogsDebugResult(this.context, params.sessionId || 'unknown', {
          success: false,
          error: 'npm-start.log not found - is the system running?'
        });
      }

      // Read entire log file
      const content = await fs.readFile(npmStartLog, 'utf8');
      const allLines = content.split('\n').filter(line => line.trim());

      // Apply grep filter FIRST (if provided)
      let filteredLines = allLines;
      if (params.filterPattern) {
        const pattern = new RegExp(params.filterPattern, 'i');
        filteredLines = allLines.filter(line => pattern.test(line));
      }

      // Apply error filter (if requested)
      if (params.includeErrorsOnly) {
        filteredLines = filteredLines.filter(line =>
          line.includes('❌') ||
          line.toLowerCase().includes('error') ||
          line.toLowerCase().includes('failed')
        );
      }

      // Take last N lines AFTER filtering
      const tailLines = params.tailLines || 50;
      const outputLines = filteredLines.slice(-tailLines);

      // Convert to LogEntry format
      const logEntries: LogEntry[] = outputLines.map(line => ({
        timestamp: new Date().toISOString(),
        level: this.detectLogLevel(line),
        message: line,
        source: 'npm-start.log',
        rawLine: line
      }));

      return createLogsDebugResult(this.context, params.sessionId || 'unknown', {
        success: true,
        currentSession: 'current',
        logFiles: [{
          path: npmStartLog,
          size: content.length,
          exists: true,
          lastModified: new Date().toISOString(),
          canRead: true
        }],
        logEntries,
        totalLines: allLines.length,
        filteredLines: filteredLines.length,
        systemStatus: {
          serverRunning: true,
          browserConnected: false,
          sessionsActive: 0
        },
        errorSummary: { totalErrors: 0, recentErrors: [], criticalIssues: [] }
      });

    } catch (error) {
      return createLogsDebugResult(this.context, params.sessionId || 'unknown', {
        success: false,
        error: `Failed to read logs: ${error}`
      });
    }
  }

  private async findCurrentSession(debugging: any): Promise<string> {
    try {
      // Look for current session symlink or newest session
      const sessionsPath = path.join(process.cwd(), '.continuum', 'sessions', 'user', 'shared');
      
      try {
        const sessions = await fs.readdir(sessionsPath);
        if (sessions.length > 0) {
          // Return most recent session (they're UUIDs, so we'd need to check timestamps)
          // For now, return the first one found
          return sessions[0];
        }
      } catch (error) {
        debugging.warnings.push(`⚠️ Could not read sessions directory: ${error}`);
      }
      
      // Fallback: look for currentUser symlink mentioned in CLAUDE.md
      const currentUserPath = path.join(process.cwd(), '.continuum', 'jtag', 'currentUser');
      try {
        const stats = await fs.stat(currentUserPath);
        if (stats.isDirectory() || stats.isSymbolicLink()) {
          return 'currentUser';
        }
      } catch (error) {
        debugging.warnings.push(`⚠️ currentUser symlink not found: ${error}`);
      }
      
      return 'no-session-found';
    } catch (error) {
      debugging.errors.push(`❌ Error finding current session: ${error}`);
      return 'error-finding-session';
    }
  }

  private async discoverLogFiles(sessionId: string, params: LogsDebugParams, debugging: any): Promise<LogFileInfo[]> {
    const logFiles: LogFileInfo[] = [];

    const basePaths = [
      // Session-specific logs
      path.join(process.cwd(), '.continuum', 'sessions', 'user', 'shared', sessionId, 'logs'),
      // Current user logs (symlink - default path)
      path.join(process.cwd(), '.continuum', 'jtag', 'currentUser', 'logs'),
      // System logs
      path.join(process.cwd(), '.continuum', 'jtag', 'system', 'logs'),
      // Performance logs
      path.join(process.cwd(), '.continuum', 'jtag', 'performance', 'logs'),
      // General logs
      path.join(process.cwd(), '.continuum', 'jtag', 'logs')
    ];

    // ENHANCED: Include browser logs from examples (widget-ui/test-bench)
    const examplePaths = [
      'examples/widget-ui/.continuum/jtag/sessions/system',
      'examples/test-bench/.continuum/jtag/sessions/system'
    ];

    for (const examplePath of examplePaths) {
      try {
        const fullExamplePath = path.join(process.cwd(), examplePath);
        const sessions = await fs.readdir(fullExamplePath);
        for (const session of sessions) {
          const sessionLogPath = path.join(fullExamplePath, session, 'logs');
          basePaths.push(sessionLogPath);
          debugging.logs.push(`📁 Added browser session logs: ${sessionLogPath}`);
        }
      } catch (error) {
        // Not a problem if examples don't exist
        debugging.logs.push(`📋 No example logs found in ${examplePath}: ${error}`);
      }
    }

    for (const basePath of basePaths) {
      try {
        const files = await fs.readdir(basePath);

        for (const file of files) {
          // Enhanced: Support all log file types (.log, .json, .txt, .out, .err)
          if (this.isLogFile(file)) {
            const filePath = path.join(basePath, file);
            const logFileInfo = await this.getLogFileInfo(filePath, debugging);
            if (logFileInfo) {
              logFiles.push(logFileInfo);
              debugging.logs.push(`📄 Found log file: ${path.basename(filePath)} (${logFileInfo.size} bytes)`);
            }
          }
        }
      } catch (error) {
        debugging.warnings.push(`⚠️ Could not read log directory ${basePath}: ${error}`);
      }
    }

    debugging.logs.push(`✅ Total discovered log files: ${logFiles.length}`);
    return logFiles;
  }

  /**
   * Enhanced log file detection - supports all JTAG log types
   */
  private isLogFile(filename: string): boolean {
    const logExtensions = ['.log', '.json', '.txt', '.out', '.err'];
    const logPatterns = [
      /^.*-console-.*\.(log|json)$/,  // browser-console-log.log, server-console-debug.json
      /^npm-start\.log$/,              // npm-start.log
      /^.*\.log$/,                     // Any .log file
      /^.*\.log\.json$/,               // Any .log.json file
      /^.*\.err$/,                     // Error logs
      /^.*\.out$/                      // Output logs
    ];

    // Check extensions
    for (const ext of logExtensions) {
      if (filename.endsWith(ext)) {
        return true;
      }
    }

    // Check patterns
    for (const pattern of logPatterns) {
      if (pattern.test(filename)) {
        return true;
      }
    }

    return false;
  }

  private async getLogFileInfo(filePath: string, debugging: any): Promise<LogFileInfo | null> {
    try {
      const stats = await fs.stat(filePath);
      
      // Check if readable
      let canRead = true;
      try {
        await fs.access(filePath, fsConstants.R_OK);
      } catch {
        canRead = false;
      }
      
      return {
        path: filePath,
        size: stats.size,
        exists: true,
        lastModified: stats.mtime.toISOString(),
        canRead
      };
    } catch (error) {
      debugging.warnings.push(`⚠️ Error getting log file info for ${filePath}: ${error}`);
      return null;
    }
  }

  private async readLogEntries(logFiles: LogFileInfo[], params: LogsDebugParams, debugging: any): Promise<{
    logEntries: LogEntry[];
    totalLines: number;
    filteredLines: number;
  }> {
    const allEntries: LogEntry[] = [];
    let totalLines = 0;
    
    const tailLines = params.tailLines || 100;
    
    for (const logFile of logFiles.filter(f => f.canRead)) {
      try {
        const content = await fs.readFile(logFile.path, 'utf8');
        const lines = content.split('\n').filter(line => line.trim());
        totalLines += lines.length;
        
        // Take last N lines if tailing
        const relevantLines = lines.slice(-tailLines);
        
        for (const line of relevantLines) {
          const entry = this.parseLogLine(line, logFile.path);
          if (entry && this.matchesFilter(entry, params)) {
            allEntries.push(entry);
          }
        }
      } catch (error) {
        debugging.warnings.push(`⚠️ Error reading ${logFile.path}: ${error}`);
      }
    }
    
    // Sort by timestamp (most recent first)
    allEntries.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
    
    return {
      logEntries: allEntries,
      totalLines,
      filteredLines: allEntries.length
    };
  }

  private parseLogLine(line: string, source: string): LogEntry | null {
    try {
      // Try to parse as JSON first (structured logs)
      const jsonMatch = line.match(/\{.*\}/);
      if (jsonMatch) {
        try {
          const parsed = JSON.parse(jsonMatch[0]);
          return {
            timestamp: parsed.timestamp || new Date().toISOString(),
            level: this.detectLogLevel(parsed.level || line),
            message: parsed.message || line,
            source: path.basename(source),
            rawLine: line
          };
        } catch {
          // Fall through to text parsing
        }
      }
      
      // Parse text-based logs
      const level = this.detectLogLevel(line);
      const timestamp = this.extractTimestamp(line) || new Date().toISOString();
      
      return {
        timestamp,
        level,
        message: line,
        source: path.basename(source),
        rawLine: line
      };
    } catch {
      return null;
    }
  }

  private detectLogLevel(text: string): 'error' | 'warn' | 'info' | 'debug' {
    const lowerText = text.toLowerCase();
    if (lowerText.includes('error') || lowerText.includes('❌')) return 'error';
    if (lowerText.includes('warn') || lowerText.includes('⚠️')) return 'warn';
    if (lowerText.includes('debug') || lowerText.includes('🔍')) return 'debug';
    return 'info';
  }

  private extractTimestamp(line: string): string | null {
    // Look for ISO timestamp
    const isoMatch = line.match(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{3})?Z?/);
    if (isoMatch) return isoMatch[0];
    
    // Look for other common timestamp formats
    const dateMatch = line.match(/\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}/);
    if (dateMatch) return new Date(dateMatch[0]).toISOString();
    
    return null;
  }

  private matchesFilter(entry: LogEntry, params: LogsDebugParams): boolean {
    if (params.includeErrorsOnly && entry.level !== 'error') {
      return false;
    }
    
    if (params.filterPattern) {
      const pattern = new RegExp(params.filterPattern, 'i');
      if (!pattern.test(entry.message)) {
        return false;
      }
    }
    
    return true;
  }

  private async analyzeSystemStatus(debugging: any): Promise<any> {
    // Check if server processes are running, count sessions, etc.
    // This is server-specific analysis
    return {
      serverRunning: true, // We're running this command, so server is up
      browserConnected: false, // Would need to check WebSocket connections
      sessionsActive: 0 // Would need to count active sessions
    };
  }

  private generateErrorSummary(logEntries: LogEntry[], debugging: any): any {
    const errors = logEntries.filter(entry => entry.level === 'error');
    const recentErrors = errors.slice(0, 10); // Last 10 errors
    
    // Analyze critical issues
    const criticalIssues: string[] = [];
    for (const error of errors) {
      if (error.message.includes('Send failed: undefined')) {
        criticalIssues.push('ChatWidget sendMessage failing with undefined error');
      }
      if (error.message.includes('Widget not found')) {
        criticalIssues.push('Widget DOM elements not accessible');
      }
      if (error.message.includes('Event system')) {
        criticalIssues.push('Real-time event system broken');
      }
    }
    
    return {
      totalErrors: errors.length,
      recentErrors,
      criticalIssues: Array.from(new Set(criticalIssues)) // Remove duplicates
    };
  }
}