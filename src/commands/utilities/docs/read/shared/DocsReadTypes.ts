import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { Commands } from '../../../../../system/core/shared/Commands';
import { createPayload, transformPayload } from '@system/core/types/JTAGTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { SYSTEM_SCOPES } from '@system/core/types/SystemScopes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Reads the content of a documentation file by name, with support for table-of-contents extraction, jumping to a specific section, or reading a line range.
 */
export interface DocsReadParams extends CommandParams {
  doc: string;           // Simple doc name from docs/list
  toc?: boolean;         // Return table of contents with line ranges
  section?: string;      // Jump to specific section by title
  startLine?: number;
  endLine?: number;
}

export interface SectionInfo {
  level: number;         // 1 for #, 2 for ##, etc.
  title: string;
  lines: [number, number];  // [startLine, endLine]
}

export interface DocsReadResult extends CommandResult {
  success: boolean;
  error?: string;
  doc: string;
  content?: string;      // Only if not --toc only
  toc?: SectionInfo[];   // Only if --toc flag
  totalLines: number;
}

/**
 * DocsRead — Type-safe command executor
 *
 * Usage:
 *   import { DocsRead } from '...shared/DocsReadTypes';
 *   const result = await DocsRead.execute({ ... });
 */
export const DocsRead = {
  execute(params: CommandInput<DocsReadParams>): Promise<DocsReadResult> {
    return Commands.execute<DocsReadParams, DocsReadResult>('utilities/docs/read', params as Partial<DocsReadParams>);
  },
  commandName: 'utilities/docs/read' as const,
} as const;

/**
 * Factory function for creating UtilitiesDocsReadParams
 */
export const createUtilitiesDocsReadParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DocsReadParams, 'context' | 'sessionId' | 'userId'>
): DocsReadParams => createPayload(context, sessionId, {
  userId: SYSTEM_SCOPES.SYSTEM,
  ...data
});

/**
 * Factory function for creating UtilitiesDocsReadResult with defaults
 */
export const createUtilitiesDocsReadResult = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<DocsReadResult, 'context' | 'sessionId' | 'userId'>
): DocsReadResult => createPayload(context, sessionId, {
  ...data
});

/**
 * Smart utilities/docs/read-specific inheritance from params
 * Auto-inherits context and sessionId from params
 * Must provide all required result fields
 */
export const createUtilitiesDocsReadResultFromParams = (
  params: DocsReadParams,
  differences: Omit<DocsReadResult, 'context' | 'sessionId' | 'userId'>
): DocsReadResult => transformPayload(params, differences);

