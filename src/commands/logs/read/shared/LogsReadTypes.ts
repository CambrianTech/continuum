import type { CommandParams, JTAGContext, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../system/core/shared/Commands';

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

export interface LogsReadResult {
  context: JTAGContext;
  sessionId: UUID;
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
