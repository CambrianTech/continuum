/**
 * Genome Academy Session Restart Command - Shared Types
 *
 * Restarts a completed or failed Academy session by cloning the original
 * session's configuration and launching a fresh teacher/student sentinel pair.
 */

import type { CommandParams, CommandResult, CommandInput } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@system/core/types/CrossPlatformUUID';

/**
 * Genome Academy Session Restart Command Parameters
 */
export interface GenomeAcademySessionRestartParams extends CommandParams {
  /** The original academy session ID to clone configuration from */
  sessionId: string;
}

/**
 * Genome Academy Session Restart Command Result
 */
export interface GenomeAcademySessionRestartResult extends CommandResult {
  success: boolean;
  /** The newly created Academy session ID */
  newSessionId: string;
  /** Sentinel handle for the new teacher pipeline */
  teacherHandle: string;
  /** Sentinel handle for the new student pipeline */
  studentHandle: string;
  error?: string;
}

/**
 * Smart inheritance from params — auto-inherits context and sessionId
 */
export const createGenomeAcademySessionRestartResultFromParams = (
  params: GenomeAcademySessionRestartParams,
  differences: Omit<GenomeAcademySessionRestartResult, 'context' | 'sessionId' | 'userId'>
): GenomeAcademySessionRestartResult => transformPayload(params, differences);

/**
 * GenomeAcademySessionRestart — Type-safe command executor
 */
export const GenomeAcademySessionRestart = {
  execute(params: CommandInput<GenomeAcademySessionRestartParams>): Promise<GenomeAcademySessionRestartResult> {
    return Commands.execute<GenomeAcademySessionRestartParams, GenomeAcademySessionRestartResult>(
      'genome/academy-session-restart',
      params as Partial<GenomeAcademySessionRestartParams>
    );
  },
  commandName: 'genome/academy-session-restart' as const,
} as const;
