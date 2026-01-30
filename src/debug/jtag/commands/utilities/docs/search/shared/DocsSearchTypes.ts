import type { CommandParams, JTAGContext, CommandInput} from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { Commands } from '../../../../../system/core/shared/Commands';

export interface DocsSearchParams extends CommandParams {
  pattern: string;
  caseSensitive?: boolean;
  maxMatches?: number;
}

export interface DocsSearchResult {
  context: JTAGContext;
  sessionId: UUID;
  success: boolean;
  error?: string;
  pattern: string;
  matches: Array<{ doc: string; lineNumber: number; content: string }>;
  totalMatches: number;
}

/**
 * DocsSearch — Type-safe command executor
 *
 * Usage:
 *   import { DocsSearch } from '...shared/DocsSearchTypes';
 *   const result = await DocsSearch.execute({ ... });
 */
export const DocsSearch = {
  execute(params: CommandInput<DocsSearchParams>): Promise<DocsSearchResult> {
    return Commands.execute<DocsSearchParams, DocsSearchResult>('utilities/docs/search', params as Partial<DocsSearchParams>);
  },
  commandName: 'utilities/docs/search' as const,
} as const;
