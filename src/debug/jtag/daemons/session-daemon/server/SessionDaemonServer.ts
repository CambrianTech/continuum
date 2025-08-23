/**
 * Session Daemon - Server Implementation
 * 
 * Server-specific session daemon that handles session identity management.
 * Follows the sparse override pattern - minimal server-specific logic.
 */

import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { SessionDaemon} from '../shared/SessionDaemon';
import {  
  type SessionMetadata, 
  type CreateSessionParams, 
  type CreateSessionResult, 
  type SessionResponse,
  type SessionErrorResponse,
  type SessionOperation,
  type GetSessionParams,
  type GetSessionResult,
  type ListSessionsParams,
  type ListSessionsResult,
} from '../shared/SessionTypes';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import { createPayload } from '../../../system/core/types/JTAGTypes';
import { type JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { WorkingDirConfig } from '../../../system/core/config/WorkingDirConfig';
import fs from 'fs/promises';
import path from 'path';

const createSessionErrorResponse = (
  error: string,
  context: JTAGContext,
  sessionId: UUID,
  operation?: SessionOperation
): SessionErrorResponse => {
  return createPayload(context, sessionId, {
    operation,
    success: false,
    timestamp: new Date().toISOString(),
    error
  });
};

export class SessionDaemonServer extends SessionDaemon {
  private sessions: SessionMetadata[] = []; // In-memory active sessions for server

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Get the sessions metadata file path for the current working directory context
   */
  private getSessionsMetadataPath(): string {
    const continuumPath = WorkingDirConfig.getContinuumPath();
    return path.join(continuumPath, 'jtag', 'sessions', 'metadata.json');
  }

  /**
   * Ensure session directories exist for the current working directory context
   */
  private async ensureSessionDirectories(): Promise<void> {
    const continuumPath = WorkingDirConfig.getContinuumPath();
    const sessionDir = path.join(continuumPath, 'jtag', 'sessions');
    const logsDir = path.join(continuumPath, 'jtag', 'logs');
    const screenshotsDir = path.join(continuumPath, 'jtag', 'screenshots');
    const signalsDir = path.join(continuumPath, 'jtag', 'signals');
    
    await Promise.all([
      fs.mkdir(sessionDir, { recursive: true }),
      fs.mkdir(logsDir, { recursive: true }),
      fs.mkdir(screenshotsDir, { recursive: true }),
      fs.mkdir(signalsDir, { recursive: true })
    ]);
  }

  /**
   * Load sessions from per-project metadata file
   */
  private async loadSessionsFromFile(): Promise<void> {
    try {
      const metadataPath = this.getSessionsMetadataPath();
      const metadataContent = await fs.readFile(metadataPath, 'utf-8');
      const metadata = JSON.parse(metadataContent);
      
      if (metadata.sessions && Array.isArray(metadata.sessions)) {
        this.sessions = metadata.sessions.map((session: any) => ({
          ...session,
          created: new Date(session.created),
          lastActive: new Date(session.lastActive)
        }));
        console.log(`📖 ${this.toString()}: Loaded ${this.sessions.length} sessions from ${metadataPath}`);
      }
    } catch (error) {
      // File doesn't exist or is invalid, start with empty sessions
      console.log(`📝 ${this.toString()}: No existing session metadata found, starting fresh`);
      this.sessions = [];
    }
  }

  /**
   * Save sessions to per-project metadata file
   */
  private async saveSessionsToFile(): Promise<void> {
    try {
      await this.ensureSessionDirectories();
      const metadataPath = this.getSessionsMetadataPath();
      
      const metadata = {
        projectContext: WorkingDirConfig.getWorkingDir(),
        sessions: this.sessions,
        lastUpdated: new Date().toISOString(),
        version: '1.0.0'
      };
      
      await fs.writeFile(metadataPath, JSON.stringify(metadata, null, 2));
      console.log(`💾 ${this.toString()}: Saved ${this.sessions.length} sessions to ${metadataPath}`);
    } catch (error) {
      console.error(`❌ ${this.toString()}: Failed to save session metadata:`, error);
    }
  }

  /**
   * Initialize session daemon server with per-project session loading
   */
  protected async initialize(): Promise<void> {
    await super.initialize();
    await this.loadSessionsFromFile();
    console.log(`🏷️ ${this.toString()}: Session daemon server initialized with per-project persistence`);
  }

  /**
   * Extract session operation from endpoint path (similar to CommandDaemon.extractCommand)
   */
  private extractOperation(endpoint: string): SessionOperation {
    // endpoint format: "session-daemon/get-default" or "server/session-daemon/current"
    const parts = endpoint.split('/');
    
    // Find the 'session-daemon' segment and extract everything after it
    const sessionIndex = parts.findIndex(part => part === 'session-daemon');
    if (sessionIndex === -1 || sessionIndex === parts.length - 1) {
      // If no operation specified, default to create
      return 'create';
    }
    
    // Return everything after 'session-daemon' joined with '/'
    // e.g., "session-daemon/get-default" -> "get-default"
    return parts.slice(sessionIndex + 1).join('/') as SessionOperation;
  }

  // Only source of truth in all daemons is here:  handleMessage(message: JTAGMessage): Promise<JTAGResponsePayload>
  async handleMessage(message: JTAGMessage): Promise<SessionResponse> {
      console.log(`📨 ${this.toString()}: Handling message to ${message.endpoint}`);
      
      // Extract session operation from endpoint (similar to CommandDaemon pattern)
      const operation = this.extractOperation(message.endpoint);
      const requestPayload = message.payload;
      const requestContext = requestPayload.context ?? this.context;
      const requestSessionId = requestPayload.sessionId;
      
      if (!requestSessionId) {
        return createSessionErrorResponse(`Missing sessionId for operation: ${operation}`, requestContext, requestSessionId);
      }
      
      try {
        // Route based on extracted operation from endpoint
        switch (operation) {
          case 'create':
            return await this.createOrGetSession(requestPayload as CreateSessionParams);
          case 'get':
            return await this.getSession(requestPayload as GetSessionParams);
          case 'list':
            return await this.listSessions(requestPayload as ListSessionsParams);
          default:
            console.warn(`⚠️ ${this.toString()}: Unknown session operation: ${operation}`);
            return createSessionErrorResponse(`Unknown session operation: ${operation}`, requestContext, requestSessionId);
        }
      } catch (error) {
        const errorMessage = (error && typeof error === 'object' && 'message' in error)
          ? (error as { message: string }).message
          : String(error);
        console.error(`❌ ${this.toString()}: Error processing session operation ${operation}:`, errorMessage);
        return createSessionErrorResponse(errorMessage, requestContext, requestSessionId);
      }
    }

    public async createOrGetSession(params: CreateSessionParams): Promise<CreateSessionResult | GetSessionResult> {
        if (params.isShared) {
          // Check for existing shared session
          const existingSession = this.sessions.find(s => s.isShared && s.isActive);
          if (existingSession) {
            console.log(`⚡ ${this.toString()}: Reusing existing shared session:`, existingSession);
            return createPayload(params.context, params.sessionId, {
              success: true,
              timestamp: new Date().toISOString(),
              operation: 'get',
              session: existingSession
            });
          }
        }
        return await this.createSession(params);
    }

    private async createSession(params: CreateSessionParams): Promise<CreateSessionResult> {
      console.log(`⚡ ${this.toString()}: Creating new session:`, params);
      
      const newSession = {
        sourceContext: params.context,
        sessionId: generateUUID(),
        category: params.category,
        displayName: params.displayName,
        userId: params.userId ?? generateUUID(),
        created: new Date(),
        lastActive: new Date(),
        isActive: true,
        isShared: params.isShared
      };

      console.log(`✅ ${this.toString()}: New session created:`, newSession);

      this.sessions.push(newSession);
      
      // Persist session to per-project metadata file
      await this.saveSessionsToFile();

      return createPayload(params.context, params.sessionId, {
        success: true,
        timestamp: new Date().toISOString(),
        operation: params.operation,
        session: newSession
      });
    }

    private async getSession(params: GetSessionParams): Promise<GetSessionResult> {
      console.log(`⚡ ${this.toString()}: Getting session with ID: ${params.sessionId}`);

      const session = this.sessions.find(s => s.sessionId === params.sessionId);
      
      return createPayload(params.context, params.sessionId, {
        success: true,
        timestamp: new Date().toISOString(),
        operation: 'get',
        session
      });
    }

  private async listSessions(payload: ListSessionsParams): Promise<ListSessionsResult> {
    console.log(`⚡ ${this.toString()}: Listing sessions with filter:`, payload.filter);
    
    let sessions = this.sessions;
    const filter = payload.filter;

    if (filter?.category) {
      sessions = sessions.filter(s => s.category === filter.category);
    }
    if (filter?.isActive !== undefined) {
      sessions = sessions.filter(s => s.isActive === filter.isActive);
    }
    
    return createPayload(payload.context, payload.sessionId, {
      success: true,
      timestamp: new Date().toISOString(),
      operation: 'list',
      sessions
    });
  }   
}
