/**
 * Session Parameters - Consistent structure for all session-related operations
 * 
 * This interface defines the standard parameter structure used across:
 * - Thin client (continuum script)
 * - ConnectCommand 
 * - SessionManagerDaemon
 * - CommandProcessor
 */

export interface SessionParameters {
  sessionId?: string;
  sessionType?: 'development' | 'persona' | 'portal' | 'git-hook' | 'test';
  owner?: string;
  forceNew?: boolean;
  capabilities?: string[];
  context?: string;
}

export interface SessionRequest extends SessionParameters {
  source: string;
  sessionPreference?: 'current' | 'new' | string; // 'current', 'new', or specific sessionId
}

/**
 * Convert SessionParameters to SessionRequest with defaults
 */
export function toSessionRequest(params: SessionParameters, source: string = 'continuum-cli'): SessionRequest {
  const { sessionId, forceNew, ...rest } = params;
  
  // Determine sessionPreference based on parameters
  let sessionPreference: string;
  if (forceNew) {
    sessionPreference = 'new';
  } else if (sessionId) {
    sessionPreference = sessionId;
  } else {
    sessionPreference = 'current'; // Default to shared session
  }

  return {
    source,
    sessionPreference,
    sessionType: 'development',
    capabilities: ['browser', 'commands', 'screenshots'],
    context: 'development',
    ...rest
  };
}