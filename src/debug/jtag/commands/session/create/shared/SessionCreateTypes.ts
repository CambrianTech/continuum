/**
 * Session Create Command Types - Shared
 * 
 * Command interface for creating sessions via the session daemon.
 * Handles session creation by routing to SessionDaemon internally.
 */

import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { SessionCategory, SessionMetadata } from '../../../../daemons/session-daemon/shared/SessionTypes';

/**
 * Session create command parameters
 */
export interface SessionCreateParams extends JTAGPayload {
  /** Session category */
  category: SessionCategory;
  /** Display name for the session */
  displayName: string;
  /** Optional user ID - will generate if not provided */
  userId?: UUID;
  /** Whether this should be a shared session */
  isShared?: boolean;
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
 */
export function createSessionCreateParams(
  context: JTAGContext,
  sessionId: UUID,
  options: Partial<SessionCreateParams> = {}
): SessionCreateParams {
  return {
    context,
    sessionId,
    category: options.category || 'user',
    displayName: options.displayName || 'Default Session',
    userId: options.userId,
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