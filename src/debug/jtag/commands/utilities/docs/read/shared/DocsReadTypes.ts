import type { CommandParams, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

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

export interface DocsReadResult {
  context: JTAGContext;
  sessionId: UUID;
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
