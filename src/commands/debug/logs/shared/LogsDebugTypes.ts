/**
 * Logs Debug Command Types - System log inspection
 * 
 * Elegant debug command for current user logs and system feedback
 * Replaces raw bash commands with proper JTAG file system access
 */

import type { CommandParams, CommandResult, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import { createPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface LogsDebugParams extends CommandParams {
  logType?: 'server' | 'browser' | 'system' | 'all';
  tailLines?: number;
  includeErrorsOnly?: boolean;
  filterPattern?: string;
  startTime?: string;
  endTime?: string;
}

export interface LogEntry {
  timestamp: string;
  level: 'error' | 'warn' | 'info' | 'debug';
  message: string;
  source: string;
  rawLine: string;
}

export interface LogFileInfo {
  path: string;
  size: number;
  exists: boolean;
  lastModified: string;
  canRead: boolean;
}

export interface LogsDebugResult extends CommandResult {
  success: boolean;
  currentSession: string;
  logFiles: LogFileInfo[];
  logEntries: LogEntry[];
  totalLines: number;
  filteredLines: number;
  
  // System feedback
  systemStatus: {
    serverRunning: boolean;
    browserConnected: boolean;
    sessionsActive: number;
  };
  
  // Error summary
  errorSummary: {
    totalErrors: number;
    recentErrors: LogEntry[];
    criticalIssues: string[];
  };
  
  debugging: {
    logs: string[];
    warnings: string[];
    errors: string[];
  };
  
  error?: string;
}

export const createLogsDebugResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<Partial<LogsDebugResult>, 'context' | 'sessionId'>
): LogsDebugResult => createPayload(context, sessionId, {
  success: false,
  currentSession: '',
  logFiles: [],
  logEntries: [],
  totalLines: 0,
  filteredLines: 0,
  systemStatus: {
    serverRunning: false,
    browserConnected: false,
    sessionsActive: 0
  },
  errorSummary: {
    totalErrors: 0,
    recentErrors: [],
    criticalIssues: []
  },
  debugging: {
    logs: [],
    warnings: [],
    errors: []
  },
  ...data
});