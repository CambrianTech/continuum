/**
 * Session Create Command Types - Shared
 * 
 * Command interface for creating sessions via the session daemon.
 * Handles session creation by routing to SessionDaemon internally.
 */

import type { JTAGContext, CommandParams, JTAGPayload, CommandInput} from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { SYSTEM_SCOPES } from '../../../../system/core/types/SystemScopes';
import type { SessionCategory, SessionMetadata, EnhancedConnectionContext } from '../../../../daemons/session-daemon/shared/SessionTypes';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Session create command parameters
 */
export interface SessionCreateParams extends CommandParams {
  /** Session category */
  category: SessionCategory;
  /** Display name for the session */
  displayName: string;
  /** Whether this should be a shared session */
  isShared?: boolean;
  /** REQUIRED: Enhanced connection context with clientType and identity - determines user resolution */
  connectionContext: EnhancedConnectionContext;
}

/**
 * Session create command result
 */
export interface SessionCreateResult extends JTAGPayload {
  /** Success status */
  success: boolean;
  /** Created session metadata */
  session?: SessionMetadata;
  /** Any error message */
  error?: string;
}

/**
 * Create session create parameters with defaults
 * @param connectionContext REQUIRED - must provide valid connection context with clientType
 */
export function createSessionCreateParams(
  context: JTAGContext,
  sessionId: UUID,
  connectionContext: EnhancedConnectionContext,
  options: Partial<Omit<SessionCreateParams, 'context' | 'sessionId' | 'connectionContext'>> = {}
): SessionCreateParams {
  return {
    context,
    sessionId,
    connectionContext,
    category: options.category || 'user',
    displayName: options.displayName || 'Default Session',
    userId: options.userId ?? SYSTEM_SCOPES.SYSTEM,
    isShared: options.isShared ?? true,
    ...options
  };
}

/**
 * Create session create result from execution data
 */
export function createSessionCreateResult(
  params: SessionCreateParams,
  data: {
    success: boolean;
    session?: SessionMetadata;
    error?: string;
  }
): SessionCreateResult {
  return {
    context: params.context,
    sessionId: params.sessionId,
    success: data.success,
    session: data.session,
    error: data.error
  };
}
/**
 * SessionCreate — Type-safe command executor
 *
 * Usage:
 *   import { SessionCreate } from '...shared/SessionCreateTypes';
 *   const result = await SessionCreate.execute({ ... });
 */
export const SessionCreate = {
  execute(params: CommandInput<SessionCreateParams>): Promise<SessionCreateResult> {
    return Commands.execute<SessionCreateParams, SessionCreateResult>('session/create', params as Partial<SessionCreateParams>);
  },
  commandName: 'session/create' as const,
} as const;
