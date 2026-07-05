import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Lists all available documentation files in the project, returning metadata such as file size, line count, section headings, and directory, with optional filtering by directory or filename pattern.
 */
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

export interface DocsListResult extends CommandResult {
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

/**
 * Factory function for creating UtilitiesDocsListParams
 */
export const createUtilitiesDocsListParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DocsListParams, 'context' | 'sessionId' | 'userId'>
): DocsListParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating UtilitiesDocsListResult with defaults
 */
export const createUtilitiesDocsListResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DocsListResult, 'context' | 'sessionId' | 'userId'>
): DocsListResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart utilities/docs/list-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createUtilitiesDocsListResultFromParams = (
  params: DocsListParams,
  differences: Omit<DocsListResult, 'context' | 'sessionId' | 'userId'>
): DocsListResult => transformPayload(params, differences);

