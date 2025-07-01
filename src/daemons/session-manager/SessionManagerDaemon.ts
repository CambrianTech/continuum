/**
 * Session Manager Daemon - Core session isolation and artifact management
 * 
 * RESPONSIBILITIES:
 * - Manage isolated sessions across all system components
 * - Coordinate session lifecycle with other daemons
 * - Handle connection identity and session routing
 * - Organize artifacts by session type and owner
 * - Provide session sandboxing for safe experimentation
 * - Enable forensic analysis of session failures
 */

import { BaseDaemon } from '../base/BaseDaemon.js';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol.js';
import * as fs from 'fs/promises';
import * as path from 'path';

export interface ConnectionIdentity {
  type: 'portal' | 'validation' | 'user' | 'persona';
  name: string; // portal, git-hook, username, persona-name
  sessionContext?: string; // additional context like branch name for git hooks
}

export interface BrowserSession {
  id: string;
  type: 'persona' | 'portal' | 'git-hook' | 'development' | 'test';
  owner: string;
  created: Date;
  lastActive: Date;
  
  // Process tracking
  processes: {
    browser?: { pid: number; url: string };
    devtools?: { pid: number; tabs: string[] };
  };
  
  // Artifact storage
  artifacts: {
    storageDir: string;
    logs: { server: string[]; client: string[] };
    screenshots: string[];
    files: string[];
    recordings: string[];
    devtools: string[];
  };
  
  // Session state
  isActive: boolean;
  shouldAutoCleanup: boolean;
  cleanupAfterMs: number;
}

export interface SessionEvent {
  type: 'session_created' | 'session_joined' | 'session_closed' | 'connection_registered';
  sessionId?: string;
  connectionId?: string;
  identity?: ConnectionIdentity;
  session?: BrowserSession;
  timestamp: Date;
  metadata?: Record<string, any>;
}

export class SessionManagerDaemon extends BaseDaemon {
  public readonly name = 'session-manager';
  public readonly version = '1.0.0';
  
  private sessions = new Map<string, BrowserSession>();
  private connectionIdentities = new Map<string, ConnectionIdentity>();
  private artifactRoot: string;
  private cleanupInterval?: NodeJS.Timeout;
  private eventListeners = new Set<(event: SessionEvent) => void>();

  constructor(artifactRoot: string = '.continuum/sessions') {
    super();
    this.artifactRoot = artifactRoot;
  }

  protected async onStart(): Promise<void> {
    this.log('📋 Starting Session Manager Daemon...');
    await this.initializeDirectoryStructure();
    this.startCleanupMonitoring();
    this.log('✅ Session Manager ready for coordination');
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping Session Manager...');
    
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
    }
    
    // Close all active sessions gracefully
    const activeSessions = Array.from(this.sessions.values()).filter(s => s.isActive);
    for (const session of activeSessions) {
      await this.closeSession(session.id, { preserveArtifacts: true });
    }
  }

  /**
   * Subscribe to session events
   */
  onSessionEvent(listener: (event: SessionEvent) => void): void {
    this.eventListeners.add(listener);
    this.log(`📡 Event listener registered (${this.eventListeners.size} total)`);
  }

  /**
   * Unsubscribe from session events
   */
  offSessionEvent(listener: (event: SessionEvent) => void): void {
    this.eventListeners.delete(listener);
    this.log(`📡 Event listener removed (${this.eventListeners.size} remaining)`);
  }

  /**
   * Emit session event to all listeners
   */
  private emitEvent(event: SessionEvent): void {
    this.log(`📡 Emitting ${event.type} event (${this.eventListeners.size} listeners)`);
    
    this.eventListeners.forEach(listener => {
      try {
        listener(event);
      } catch (error) {
        this.log(`⚠️ Session event listener error: ${error}`, 'warn');
      }
    });
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    try {
      switch (message.type) {
        case 'register_connection_identity':
          return this.handleRegisterConnectionIdentity(message.data);
          
        case 'create_session':
          return await this.handleCreateSession(message.data);
          
        case 'create_session_for_connection':
          return await this.handleCreateSessionForConnection(message.data);
          
        case 'get_session':
          return this.handleGetSession(message.data);
          
        case 'list_sessions':
          return this.handleListSessions(message.data);
          
        case 'add_artifact':
          return await this.handleAddArtifact(message.data);
          
        case 'close_session':
          return await this.handleCloseSession(message.data);
          
        case 'get_session_stats':
          return this.handleGetSessionStats();

        case 'subscribe_events':
          return this.handleSubscribeEvents(message.data);
          
        default:
          return {
            success: false,
            error: `Unknown message type: ${message.type}`
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Session management failed: ${errorMessage}`
      };
    }
  }

  /**
   * Handle connection identity registration
   */
  private handleRegisterConnectionIdentity(data: any): DaemonResponse {
    const { connectionId, identity } = data;
    
    if (!connectionId || !identity) {
      return {
        success: false,
        error: 'connectionId and identity are required'
      };
    }

    this.connectionIdentities.set(connectionId, identity);
    this.log(`🆔 Registered connection: ${connectionId} as ${identity.type}/${identity.name}`);
    
    if (identity.sessionContext) {
      this.log(`📋 Session context: ${identity.sessionContext}`);
    }

    // Emit connection registered event
    this.emitEvent({
      type: 'connection_registered',
      connectionId,
      identity,
      timestamp: new Date(),
      metadata: {
        sessionContext: identity.sessionContext
      }
    });

    return {
      success: true,
      data: {
        connectionId,
        identity,
        message: 'Connection identity registered'
      }
    };
  }

  /**
   * Handle session creation
   */
  private async handleCreateSession(data: any): Promise<DaemonResponse> {
    const { type, owner, options = {} } = data;
    
    if (!type || !owner) {
      return {
        success: false,
        error: 'type and owner are required'
      };
    }

    try {
      const sessionId = await this.createSession(type, owner, options);
      
      return {
        success: true,
        data: {
          sessionId,
          session: this.sessions.get(sessionId),
          message: 'Session created successfully'
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Handle session creation for identified connection
   */
  private async handleCreateSessionForConnection(data: any): Promise<DaemonResponse> {
    const { connectionId, options = {} } = data;
    
    if (!connectionId) {
      return {
        success: false,
        error: 'connectionId is required'
      };
    }

    const identity = this.connectionIdentities.get(connectionId);
    if (!identity) {
      return {
        success: false,
        error: `Connection ${connectionId} not identified. Register identity first.`
      };
    }

    try {
      const sessionId = await this.createSession(identity.type, identity.name, {
        ...options,
        sessionContext: identity.sessionContext
      });
      
      return {
        success: true,
        data: {
          sessionId,
          session: this.sessions.get(sessionId),
          identity,
          message: 'Session created for connection'
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Handle get session request
   */
  private handleGetSession(data: any): DaemonResponse {
    const { sessionId } = data;
    
    if (!sessionId) {
      return {
        success: false,
        error: 'sessionId is required'
      };
    }

    const session = this.sessions.get(sessionId);
    
    if (!session) {
      return {
        success: false,
        error: `Session ${sessionId} not found`
      };
    }

    return {
      success: true,
      data: { session }
    };
  }

  /**
   * Handle list sessions request
   */
  private handleListSessions(data: any): DaemonResponse {
    const { filter = {} } = data;
    
    let sessions = Array.from(this.sessions.values());
    
    // Apply filters
    if (filter.type) {
      sessions = sessions.filter(s => s.type === filter.type);
    }
    if (filter.owner) {
      sessions = sessions.filter(s => s.owner === filter.owner);
    }
    if (filter.active !== undefined) {
      sessions = sessions.filter(s => s.isActive === filter.active);
    }

    return {
      success: true,
      data: {
        sessions,
        total: sessions.length,
        filter
      }
    };
  }

  /**
   * Handle add artifact request
   */
  private async handleAddArtifact(data: any): Promise<DaemonResponse> {
    const { sessionId, artifact } = data;
    
    if (!sessionId || !artifact) {
      return {
        success: false,
        error: 'sessionId and artifact are required'
      };
    }

    try {
      const artifactPath = await this.addArtifact(sessionId, artifact);
      
      return {
        success: true,
        data: {
          artifactPath,
          sessionId,
          message: 'Artifact added to session'
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Handle close session request
   */
  private async handleCloseSession(data: any): Promise<DaemonResponse> {
    const { sessionId, preserveArtifacts = true } = data;
    
    if (!sessionId) {
      return {
        success: false,
        error: 'sessionId is required'
      };
    }

    try {
      await this.closeSession(sessionId, { preserveArtifacts });
      
      return {
        success: true,
        data: {
          sessionId,
          message: 'Session closed successfully'
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: errorMessage
      };
    }
  }

  /**
   * Handle event subscription request
   */
  private handleSubscribeEvents(data: any): DaemonResponse {
    const { callbackDaemon, eventTypes } = data;
    
    if (!callbackDaemon) {
      return {
        success: false,
        error: 'callbackDaemon is required for event subscription'
      };
    }

    // Store callback info for later use
    // In a real implementation, we'd set up WebSocket or message routing
    this.log(`📡 Event subscription registered for daemon: ${callbackDaemon}`);
    
    if (eventTypes && Array.isArray(eventTypes)) {
      this.log(`📡 Subscribed to events: ${eventTypes.join(', ')}`);
    }

    return {
      success: true,
      data: {
        callbackDaemon,
        eventTypes: eventTypes || ['session_created', 'session_closed', 'connection_registered'],
        message: 'Event subscription registered'
      }
    };
  }

  /**
   * Handle get session stats request
   */
  private handleGetSessionStats(): DaemonResponse {
    const allSessions = Array.from(this.sessions.values());
    const activeSessions = allSessions.filter(s => s.isActive);
    
    const statsByType = allSessions.reduce((stats, session) => {
      stats[session.type] = (stats[session.type] || 0) + 1;
      return stats;
    }, {} as Record<string, number>);

    return {
      success: true,
      data: {
        total: allSessions.length,
        active: activeSessions.length,
        inactive: allSessions.length - activeSessions.length,
        byType: statsByType,
        connections: this.connectionIdentities.size
      }
    };
  }

  // Core session management methods (simplified versions of the previous SessionManager)
  
  private async createSession(
    type: BrowserSession['type'],
    owner: string,
    options: {
      autoCleanup?: boolean;
      cleanupAfterMs?: number;
      url?: string;
      sessionContext?: string;
    } = {}
  ): Promise<string> {
    const sessionId = this.generateSessionId(type, owner, options.sessionContext);
    const storageDir = await this.createSessionStorage(type, owner, sessionId, options.sessionContext);

    const session: BrowserSession = {
      id: sessionId,
      type,
      owner,
      created: new Date(),
      lastActive: new Date(),
      
      processes: {},
      
      artifacts: {
        storageDir,
        logs: { server: [], client: [] },
        screenshots: [],
        files: [],
        recordings: [],
        devtools: []
      },
      
      isActive: true,
      shouldAutoCleanup: options.autoCleanup ?? true,
      cleanupAfterMs: options.cleanupAfterMs ?? (2 * 60 * 60 * 1000) // 2 hours default
    };

    this.sessions.set(sessionId, session);
    this.log(`📋 Created session: ${sessionId} for ${owner} (${type})`);
    
    // Emit session created event with full session info
    this.emitEvent({
      type: 'session_created',
      sessionId,
      session,
      timestamp: new Date(),
      metadata: {
        storageDir: session.artifacts.storageDir,
        sessionContext: options.sessionContext,
        autoCleanup: session.shouldAutoCleanup
      }
    });
    
    return sessionId;
  }

  private async addArtifact(
    sessionId: string,
    artifact: {
      type: 'log' | 'screenshot' | 'file' | 'recording' | 'devtools';
      source: 'server' | 'client' | 'browser' | 'devtools';
      path: string;
      metadata?: Record<string, any>;
    }
  ): Promise<string> {
    const session = this.sessions.get(sessionId);
    if (!session) {
      throw new Error(`Session ${sessionId} not found`);
    }

    const timestamp = new Date();
    const filename = this.generateArtifactFilename(artifact.type, artifact.source, timestamp);
    const fullPath = path.join(session.artifacts.storageDir, artifact.type + 's', filename); // logs/, screenshots/, etc.

    // Add to appropriate category
    switch (artifact.type) {
      case 'log':
        if (artifact.source === 'server') {
          session.artifacts.logs.server.push(fullPath);
        } else {
          session.artifacts.logs.client.push(fullPath);
        }
        break;
      case 'screenshot':
        session.artifacts.screenshots.push(fullPath);
        break;
      case 'file':
        session.artifacts.files.push(fullPath);
        break;
      case 'recording':
        session.artifacts.recordings.push(fullPath);
        break;
      case 'devtools':
        session.artifacts.devtools.push(fullPath);
        break;
    }

    session.lastActive = new Date();
    this.log(`📎 Added ${artifact.type} artifact to session ${sessionId}: ${filename}`);
    
    return fullPath;
  }

  private async closeSession(sessionId: string, options: { preserveArtifacts?: boolean } = {}): Promise<void> {
    const session = this.sessions.get(sessionId);
    if (!session) {
      this.log(`⚠️ Session ${sessionId} not found for closure`, 'warn');
      return;
    }

    this.log(`🔒 Closing session: ${sessionId}`);

    // Close any tracked processes
    if (session.processes.browser?.pid) {
      try {
        process.kill(session.processes.browser.pid);
        this.log(`🔴 Closed browser process (PID: ${session.processes.browser.pid})`);
      } catch (error) {
        this.log(`⚠️ Failed to close browser: ${error}`, 'warn');
      }
    }

    if (session.processes.devtools?.pid) {
      try {
        process.kill(session.processes.devtools.pid);
        this.log(`🔴 Closed DevTools process (PID: ${session.processes.devtools.pid})`);
      } catch (error) {
        this.log(`⚠️ Failed to close DevTools: ${error}`, 'warn');
      }
    }

    session.isActive = false;

    // Emit session closed event before cleanup
    this.emitEvent({
      type: 'session_closed',
      sessionId,
      session,
      timestamp: new Date(),
      metadata: {
        preserveArtifacts: options.preserveArtifacts,
        storageDir: session.artifacts.storageDir
      }
    });

    // Cleanup artifacts if requested
    if (!options.preserveArtifacts) {
      await this.cleanupSessionArtifacts(session);
    }

    this.sessions.delete(sessionId);
    this.log(`✅ Session ${sessionId} closed and cleaned up`);
  }

  // Helper methods
  
  private async initializeDirectoryStructure(): Promise<void> {
    try {
      const baseDirs = ['portal', 'validation', 'user', 'personas'];
      for (const dir of baseDirs) {
        await fs.mkdir(path.join(this.artifactRoot, dir), { recursive: true });
      }
      this.log('📁 Session directory structure initialized');
    } catch (error) {
      this.log(`❌ Failed to initialize session directories: ${error}`, 'error');
    }
  }

  private generateSessionId(type: string, owner: string, context?: string): string {
    const timestamp = Date.now().toString(36);
    const random = Math.random().toString(36).substr(2, 5);
    const contextSuffix = context ? `-${context}` : '';
    return `${type}-${owner}-${timestamp}-${random}${contextSuffix}`;
  }

  private async createSessionStorage(
    type: BrowserSession['type'], 
    owner: string, 
    sessionId: string,
    sessionContext?: string
  ): Promise<string> {
    // Build organized path based on session type
    let sessionPath: string;
    
    switch (type) {
      case 'portal':
        sessionPath = path.join(this.artifactRoot, 'portal', sessionId);
        break;
      case 'git-hook':
        const branchContext = sessionContext ? `-${sessionContext}` : '';
        sessionPath = path.join(this.artifactRoot, 'validation', `${sessionId}${branchContext}`);
        break;
      case 'development':
      case 'test':
        sessionPath = path.join(this.artifactRoot, 'user', owner, sessionId);
        break;
      case 'persona':
        sessionPath = path.join(this.artifactRoot, 'personas', owner, sessionId);
        break;
      default:
        sessionPath = path.join(this.artifactRoot, 'misc', sessionId);
    }
    
    await fs.mkdir(sessionPath, { recursive: true });
    
    // Create subdirectories
    const subdirs = ['logs', 'screenshots', 'files', 'recordings', 'devtools'];
    for (const subdir of subdirs) {
      await fs.mkdir(path.join(sessionPath, subdir), { recursive: true });
    }
    
    // Create session metadata
    const metadata = {
      sessionId, type, owner, sessionContext,
      created: new Date().toISOString(),
      structure: {
        logs: 'Server and client logs',
        screenshots: 'Browser screenshots and visual artifacts', 
        files: 'Downloaded files and exports',
        recordings: 'Screen recordings and interactions',
        devtools: 'DevTools dumps and debugging artifacts'
      }
    };
    
    await fs.writeFile(
      path.join(sessionPath, 'session-info.json'), 
      JSON.stringify(metadata, null, 2)
    );
    
    return sessionPath;
  }

  private generateArtifactFilename(type: string, source: string, timestamp: Date): string {
    const dateStr = timestamp.toISOString().replace(/[:.]/g, '-');
    const extension = type === 'log' ? 'log' : type === 'screenshot' ? 'png' : 'json';
    return `${dateStr}-${source}-${type}.${extension}`;
  }

  private startCleanupMonitoring(): void {
    this.cleanupInterval = setInterval(async () => {
      const now = Date.now();
      for (const [sessionId, session] of this.sessions) {
        if (!session.shouldAutoCleanup || !session.isActive) continue;
        
        const age = now - session.lastActive.getTime();
        if (age > session.cleanupAfterMs) {
          this.log(`🧹 Auto-cleaning session: ${sessionId} (${Math.round(age / 60000)}min old)`);
          await this.closeSession(sessionId, { preserveArtifacts: false });
        }
      }
    }, 5 * 60 * 1000); // Check every 5 minutes
  }

  private async cleanupSessionArtifacts(session: BrowserSession): Promise<void> {
    try {
      await fs.rm(session.artifacts.storageDir, { recursive: true, force: true });
      this.log(`🗑️ Cleaned up artifacts for session ${session.id}`);
    } catch (error) {
      this.log(`⚠️ Failed to cleanup artifacts for ${session.id}: ${error}`, 'warn');
    }
  }
}