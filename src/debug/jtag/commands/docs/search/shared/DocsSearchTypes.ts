import type { CommandParams, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

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
