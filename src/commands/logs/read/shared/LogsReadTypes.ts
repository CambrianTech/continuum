import type { CommandParams, CommandResult, CommandInput} from '../../../../system/core/types/JTAGTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/** Read lines from a log file with optional filtering by level or component, and optional multi-dimensional structure analysis (temporal, severity, spatial). */
export interface LogsReadParams extends CommandParams {
  log: string;
  startLine?: number;
  endLine?: number;
  tail?: number;
  level?: 'DEBUG' | 'INFO' | 'WARN' | 'ERROR';
  component?: string;
  analyzeStructure?: boolean;  // Return multi-dimensional structure analysis
  includeSessionLogs?: boolean;  // Include session logs in error message (default: false)
}

export interface LogsReadResult extends CommandResult {
  success: boolean;
  error?: string;
  log: string;
  lines: LogLine[];
  totalLines: number;
  hasMore: boolean;
  nextLine?: number;
  structure?: LogStructureAnalysis;  // Only if analyzeStructure=true
}

export interface LogLine {
  lineNumber: number;
  content: string;
  timestamp?: string;
  level?: string;
  component?: string;
}

// Structure Analysis Types (Phase 1)
export interface LogStructureAnalysis {
  temporal: TemporalView;
  severity: SeverityView;
  spatial: SpatialView;
  totalLines: number;
  timeRange?: [string, string];  // [start, end] ISO timestamps
}

export interface TemporalView {
  view: 'temporal';
  segments: TemporalSegment[];
}

export interface TemporalSegment {
  start: string;      // ISO timestamp or time string
  end: string;
  lines: [number, number];  // [startLine, endLine]
  eventCount: number;
}

export interface SeverityView {
  view: 'severity';
  levels: SeverityLevel[];
}

export interface SeverityLevel {
  level: 'DEBUG' | 'INFO' | 'WARN' | 'ERROR';
  lines: number[];    // Array of line numbers
  eventCount: number;
}

export interface SpatialView {
  view: 'spatial';
  components: SpatialComponent[];
}

export interface SpatialComponent {
  component: string;  // Component/thread/module name
  lines: number[];    // Array of line numbers
  eventCount: number;
}

/**
 * LogsRead — Type-safe command executor
 *
 * Usage:
 *   import { LogsRead } from '...shared/LogsReadTypes';
 *   const result = await LogsRead.execute({ ... });
 */
export const LogsRead = {
  execute(params: CommandInput<LogsReadParams>): Promise<LogsReadResult> {
    return Commands.execute<LogsReadParams, LogsReadResult>('logs/read', params as Partial<LogsReadParams>);
  },
  commandName: 'logs/read' as const,
} as const;

/**
 * Factory function for creating LogsReadParams
 */
export const createLogsReadParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LogsReadParams, 'context' | 'sessionId' | 'userId'>
): LogsReadParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating LogsReadResult with defaults
 */
export const createLogsReadResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<LogsReadResult, 'context' | 'sessionId' | 'userId'>
): LogsReadResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart logs/read-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createLogsReadResultFromParams = (
  params: LogsReadParams,
  differences: Omit<LogsReadResult, 'context' | 'sessionId' | 'userId'>
): LogsReadResult => transformPayload(params, differences);

