import type { CommandParams, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface LogsReadParams extends CommandParams {
  log: string;
  startLine?: number;
  endLine?: number;
  tail?: number;
  level?: 'DEBUG' | 'INFO' | 'WARN' | 'ERROR';
  component?: string;
  analyzeStructure?: boolean;  // Return multi-dimensional structure analysis
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
