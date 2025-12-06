import type { CommandParams, JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
export interface LogsStatsParams extends CommandParams {}
export interface LogsStatsResult { context: JTAGContext; sessionId: UUID; success: boolean; error?: string; totalFiles: number; totalSizeMB: number; }
