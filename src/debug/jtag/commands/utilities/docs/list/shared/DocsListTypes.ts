import type { CommandParams, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

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

/**
 * DocsList — Type-safe command executor
 *
 * Usage:
 *   import { DocsList } from '...shared/DocsListTypes';
 *   const result = await DocsList.execute({ ... });
 */
export const DocsList = {
  execute(params: CommandInput<DocsListParams>): Promise<DocsListResult> {
    return Commands.execute<DocsListParams, DocsListResult>('utilities/docs/list', params as Partial<DocsListParams>);
  },
  commandName: 'utilities/docs/list' as const,
} as const;
