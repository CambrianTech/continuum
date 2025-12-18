/**
 * code/read command types
 */

import type { JTAGContext, JTAGEnvironment } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CodeReadResult as CodeDaemonReadResult, CodeReadOptions } from '@daemons/code-daemon/shared/CodeDaemonTypes';

/**
 * Base params for code commands
 */
export interface BaseCodeParams {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  readonly backend: JTAGEnvironment;
}

/**
 * Parameters for code/read command
 */
export interface CodeReadParams extends BaseCodeParams {
  /** File path relative to jtag root, e.g. "commands/wall/write.ts" or "system/core/shared/Events.ts" (NOT absolute paths, NOT starting with "src/") */
  readonly path: string;

  /** Start line (1-indexed, optional) */
  readonly startLine?: number;

  /** End line (1-indexed, optional) */
  readonly endLine?: number;

  /** Include file metadata */
  readonly includeMetadata?: boolean;

  /** Force bypass cache */
  readonly forceRefresh?: boolean;
}

/**
 * Result of code/read command
 */
export interface CodeReadResult extends CodeDaemonReadResult {
  readonly context: JTAGContext;
  readonly sessionId: UUID;
  readonly backend: JTAGEnvironment;
  readonly timestamp: string;
}

/**
 * Create code/read params
 */
export const createCodeReadParams = (
  context: JTAGContext,
  sessionId: UUID,
  data: Omit<CodeReadParams, 'context' | 'sessionId' | 'backend'> & { backend?: JTAGEnvironment }
): CodeReadParams => {
  return {
    context,
    sessionId,
    backend: data.backend || 'server',
    path: data.path,
    startLine: data.startLine,
    endLine: data.endLine,
    includeMetadata: data.includeMetadata,
    forceRefresh: data.forceRefresh
  };
};

/**
 * Factory function to create result
 */
export const createCodeReadResultFromParams = (
  params: CodeReadParams,
  differences: Omit<Partial<CodeReadResult>, 'context' | 'sessionId' | 'backend'>
): CodeReadResult => transformPayload(params, {
  backend: params.backend, // Explicitly copy backend from params
  success: false,
  metadata: {
    path: params.path,
    size: 0,
    lines: 0,
    linesReturned: 0,
    modified: ''
  },
  timestamp: new Date().toISOString(),
  ...differences
});
