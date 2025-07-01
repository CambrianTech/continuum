/**
 * Connection Orchestrator - Intelligent session routing
 * 
 * Implements the smart connection logic that decides whether to:
 * - Join existing session (default)
 * - Create new session
 * - Fork from existing session
 */

import { BrowserSession, SessionType } from '../SessionManagerDaemon.js';

export interface ConnectionRequest {
  source: string;
  owner: string;
  sessionPreference?: 'current' | 'new' | string; // string = sessionId or fork:sessionId
  capabilities?: string[];
  context?: string;
  type?: SessionType;
}

export interface ConnectionResult {
  sessionId: string;
  action: 'joined_existing' | 'created_new' | 'forked_from';
  launched: {
    browser: boolean;
    webserver: boolean;
    newLogFiles: boolean;
  };
  logs: {
    browser: string;
    server: string;
  };
  interface: string;
  screenshots: string;
  commands: {
    otherClients: string;
    stop: string;
    fork: string;
    info: string;
  };
}

export interface SessionProvider {
  getSession(sessionId: string): BrowserSession | null;
  getLatestSession(criteria: { owner?: string; type?: SessionType; active?: boolean }): BrowserSession | null;
  createSession(options: any): Promise<BrowserSession>;
  forkSession(fromSessionId: string, options: any): Promise<BrowserSession>;
}

export class ConnectionOrchestrator {
  constructor(private sessionProvider: SessionProvider) {}

  /**
   * Orchestrate connection and determine what action to take
   */
  async orchestrate(request: ConnectionRequest): Promise<{ success: boolean; data?: ConnectionResult; error?: string }> {
    const { source, owner, sessionPreference = 'current', capabilities = [], context = 'development', type = 'development' } = request;

    try {
      let session: BrowserSession | null = null;
      let action: 'joined_existing' | 'created_new' | 'forked_from' = 'joined_existing';
      let newLogFiles = false;

      // Apply intelligent routing logic
      if (sessionPreference === 'new' || sessionPreference.startsWith('fork:')) {
        // Force new session or fork from existing
        if (sessionPreference.startsWith('fork:')) {
          const forkFromId = sessionPreference.split(':')[1];
          session = await this.sessionProvider.forkSession(forkFromId, { owner, type, context });
          action = 'forked_from';
        } else {
          session = await this.sessionProvider.createSession({ 
            type, 
            owner, 
            context,
            starter: source,
            identity: { name: owner, user: owner }
          });
          action = 'created_new';
        }
        newLogFiles = true;
      } else if (sessionPreference !== 'current' && sessionPreference.length > 10) {
        // Specific session ID requested
        session = this.sessionProvider.getSession(sessionPreference);
        if (!session) {
          return { success: false, error: `Session ${sessionPreference} not found` };
        }
      } else {
        // Default: find or create current session
        session = this.sessionProvider.getLatestSession({
          owner,
          type,
          active: true
        });

        if (!session) {
          session = await this.sessionProvider.createSession({
            type,
            owner,
            context,
            starter: source,
            identity: { name: owner, user: owner }
          });
          action = 'created_new';
          newLogFiles = true;
        }
      }

      if (!session) {
        return { success: false, error: 'Failed to get or create session' };
      }

      // Build result with orchestration details
      const result: ConnectionResult = {
        sessionId: session.id,
        action,
        launched: {
          browser: capabilities.includes('browser'),
          webserver: true, // Always running
          newLogFiles
        },
        logs: {
          browser: session.artifacts.logs.client[0] || `${session.artifacts.storageDir}/logs/browser.log`,
          server: session.artifacts.logs.server[0] || `${session.artifacts.storageDir}/logs/server.log`
        },
        interface: 'http://localhost:9000',
        screenshots: `${session.artifacts.storageDir}/screenshots`,
        commands: {
          otherClients: `continuum session-clients ${session.id}`,
          stop: `continuum session-stop ${session.id}`,
          fork: `continuum session-fork ${session.id}`,
          info: `continuum session-info ${session.id}`
        }
      };

      return { success: true, data: result };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { success: false, error: `Connection orchestration failed: ${errorMessage}` };
    }
  }
}