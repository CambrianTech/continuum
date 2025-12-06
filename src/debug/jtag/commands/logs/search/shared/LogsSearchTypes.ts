import type { CommandParams, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
export interface LogsSearchParams extends CommandParams { pattern: string; logs?: string[]; category?: string; personaId?: string; }
export interface LogsSearchResult { context: JTAGContext; sessionId: UUID; success: boolean; error?: string; matches: any[]; totalMatches: number; }
