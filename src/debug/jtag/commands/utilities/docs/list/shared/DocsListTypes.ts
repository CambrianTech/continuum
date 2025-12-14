import type { CommandParams, JTAGContext } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

export interface DocsListParams extends CommandParams {
  dir?: string;          // Filter by directory (e.g., "daemons", "system")
  pattern?: string;      // Filter by filename pattern
  includeReadmes?: boolean;  // Include README files (default: false)
}

export interface DocMetadata {
  name: string;          // Simple name like "LOGGING" or "daemons/DAEMON-ARCHITECTURE"
  filePath: string;      // Full path
  directory: string;     // Parent directory
  sizeMB: number;
  lineCount: number;
  lastModified: string;
  sections: string[];    // H1/H2 section titles
}

export interface DocsListResult {
  context: JTAGContext;
  sessionId: UUID;
  success: boolean;
  error?: string;
  docs: DocMetadata[];
  summary: {
    totalDocs: number;
    totalSizeMB: number;
    byDirectory: Record<string, number>;
  };
}
