/**
 * logs/list Command Types
 */

import type { CommandParams, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export interface LogsListParams extends CommandParams {
  category?: 'system' | 'persona' | 'session' | 'external';
  personaId?: string;
  personaUniqueId?: string;
  component?: string;
  logType?: string;
  includeStats?: boolean;
}

export interface LogsListResult {
  context: JTAGContext;
  sessionId: UUID;
  success: boolean;
  error?: string;
  logs: LogInfo[];
  summary: {
    totalFiles: number;
    totalSizeMB: number;
    categories: Record<string, number>;
  };
}

export interface LogInfo {
  name: string;
  path: string;
  category: 'system' | 'persona' | 'session' | 'external';
  component?: string;
  personaName?: string;
  logType?: string;
  sizeMB: number;
  lineCount: number;
  lastModified: string;
  isActive: boolean;
}
