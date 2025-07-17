/**
 * Log Entry Formatter - Universal formatting for all contexts
 * Provides consistent log formatting across client, server, and remote
 */

import { BaseLogEntry, LogEntryFormatter } from './LoggingTypes';

export class UniversalLogEntryFormatter implements LogEntryFormatter {
  formatHuman(entry: BaseLogEntry): string {
    return `UL: [${entry.timestamp}] [${entry.source}] ${entry.level.toUpperCase()}: ${entry.message} [session:${entry.context.sessionId}]`;
  }

  formatJSON(entry: BaseLogEntry): string {
    return JSON.stringify({
      level: entry.level,
      message: entry.message,
      timestamp: entry.timestamp,
      source: entry.source,
      context: entry.context,
      metadata: entry.metadata
    });
  }

  formatConsole(entry: BaseLogEntry): string {
    const prefix = `[${entry.level.toUpperCase()}]`;
    const source = entry.source !== 'unknown' ? `[${entry.source}]` : '';
    const sessionInfo = entry.context.sessionId ? `[session:${entry.context.sessionId}]` : '';
    
    return `${prefix} ${source} ${entry.message} ${sessionInfo}`.trim();
  }

  /**
   * Format with execution stack information
   */
  formatWithStack(entry: BaseLogEntry): string {
    const baseFormat = this.formatHuman(entry);
    
    if (entry.context.executionStack && entry.context.executionStack.length > 0) {
      const stackInfo = entry.context.executionStack
        .map(frame => `${frame.environment}:${frame.location}${frame.description ? ` (${frame.description})` : ''}`)
        .join(' → ');
      
      return `${baseFormat} [stack: ${stackInfo}]`;
    }
    
    return baseFormat;
  }

  /**
   * Format for different output targets
   */
  formatForTarget(entry: BaseLogEntry, target: 'file' | 'console' | 'json' | 'stack'): string {
    switch (target) {
      case 'file':
        return this.formatHuman(entry);
      case 'console':
        return this.formatConsole(entry);
      case 'json':
        return this.formatJSON(entry);
      case 'stack':
        return this.formatWithStack(entry);
      default:
        return this.formatHuman(entry);
    }
  }
}