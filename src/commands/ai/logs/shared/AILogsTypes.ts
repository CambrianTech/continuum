/**
 * AI Logs Command Types
 *
 * Read and analyze the dedicated AI decision log file
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface AILogsParams {
  context?: JTAGContext;
  sessionId?: UUID;

  // Filtering
  personaName?: string;          // Filter by persona name (e.g., "Helper AI")
  decisionType?: 'RESPOND' | 'SILENT' | 'POSTED' | 'REDUNDANCY-CHECK' | 'ERROR' | 'ALL';  // Filter by decision type
  roomId?: string;                // Filter by room (short ID like "5e71a0c8")

  // Time range
  since?: string;                 // ISO timestamp or relative (e.g., "5m", "1h", "today")
  until?: string;                 // ISO timestamp

  // Output control
  tailLines?: number;             // Show last N lines (default: 50)
  includeStats?: boolean;         // Include statistics summary
  format?: 'text' | 'json';       // Output format
}

export interface AILogsResult {
  context: JTAGContext;
  sessionId: UUID;

  success: boolean;
  error?: string;

  // Log content
  logPath: string;
  lines: string[];
  totalLines: number;
  filteredLines: number;

  // Statistics (if includeStats=true)
  stats?: {
    totalDecisions: number;
    responseCount: number;
    silentCount: number;
    postedCount: number;
    redundancyChecks: number;
    redundancyDiscards: number;
    errors: number;
    personaBreakdown: {
      [personaName: string]: {
        respond: number;
        silent: number;
        posted: number;
      };
    };
  };
}
