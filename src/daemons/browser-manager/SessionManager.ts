/**
 * Browser Session Manager - Isolated sessions with artifact tracking
 * 
 * RESPONSIBILITIES:
 * - Manage independent browser sessions per persona/context
 * - Track DevTools windows separately from main browser
 * - Organize artifacts (logs, screenshots, files) by session
 * - Auto-cleanup of abandoned sessions
 * - Separate storage for git hooks, portal sessions, persona work
 */

import * as fs from 'fs/promises';
import * as path from 'path';

export interface SessionIdentity {
  starter: 'cli' | 'portal' | 'persona' | 'git-hook' | 'api' | 'test';
  name: string; // User-provided name (e.g., "joel", "main-dev", "feature-branch")
  user?: string; // Who is this? (e.g., "joel", "teammate", "ai-persona-alice")
  type?: 'development' | 'debugging' | 'testing' | 'automation' | 'collaboration';
  metadata?: {
    project?: string;
    branch?: string;
    task?: string;
    description?: string;
  };
}

export interface BrowserSession {
  id: string; // Generated: "cli-joel-main-dev-20250701-1234"
  identity: SessionIdentity;
  type: 'persona' | 'portal' | 'git-hook' | 'development' | 'test' | 'user' | 'validation';
  owner: string; // persona name, 'portal', 'git-hook', etc.
  created: Date;
  lastActive: Date;
  
  // Browser processes
  mainBrowser: {
    pid: number | null;
    url: string;
    isConnected: boolean;
  };
  
  devTools: {
    pid: number | null;
    isOpen: boolean;
    tabs: string[]; // console, network, sources, etc.
  };
  
  // Artifact storage
  artifacts: {
    storageDir: string;
    logs: {
      server: string[];
      client: string[];
    };
    screenshots: string[];
    files: string[];
    recordings: string[];
  };
  
  // Session state
  isActive: boolean;
  shouldAutoCleanup: boolean;
  cleanupAfterMs: number;
}

export interface SessionArtifact {
  type: 'log' | 'screenshot' | 'file' | 'recording';
  source: 'server' | 'client' | 'devtools';
  timestamp: Date;
  path: string;
  metadata: Record<string, any>;
}

export interface ConnectionIdentity {
  starter: 'cli' | 'portal' | 'persona' | 'git-hook' | 'api' | 'test';
  identity: SessionIdentity;
  sessionContext?: string; // additional context like branch name for git hooks
}

export interface SessionEvent {
  type: 'session_created' | 'session_joined' | 'session_closed' | 'identity_updated';
  sessionId: string;
  identity: SessionIdentity;
  timestamp: Date;
  metadata?: Record<string, any>;
}

export class SessionManager {
  private sessions = new Map<string, BrowserSession>();
  private artifactRoot: string;
  private cleanupInterval?: NodeJS.Timeout;
  private connectionIdentities = new Map<string, ConnectionIdentity>(); // connectionId -> identity
  private eventListeners = new Set<(event: SessionEvent) => void>();

  constructor(artifactRoot: string = '.continuum/sessions') {
    this.artifactRoot = artifactRoot;
    this.initializeDirectoryStructure();
    this.startCleanupMonitoring();
  }

  /**
   * Initialize proper directory structure
   */
  private async initializeDirectoryStructure(): Promise<void> {
    try {
      const baseDirs = [
        'portal',           // Portal debugging sessions
        'validation',       // Git hook validation sessions  
        'user',            // Human user sessions
        'personas'         // AI persona sessions (will have subdirs by persona name)
      ];

      for (const dir of baseDirs) {
        await fs.mkdir(path.join(this.artifactRoot, dir), { recursive: true });
      }

      console.log('📁 Session directory structure initialized');
    } catch (error) {
      console.error('❌ Failed to initialize session directories:', error);
    }
  }

  /**
   * Add event listener for session events
   */
  onSessionEvent(listener: (event: SessionEvent) => void): void {
    this.eventListeners.add(listener);
  }

  /**
   * Remove event listener
   */
  offSessionEvent(listener: (event: SessionEvent) => void): void {
    this.eventListeners.delete(listener);
  }

  /**
   * Emit session event to all listeners
   */
  private emitEvent(event: SessionEvent): void {
    this.eventListeners.forEach(listener => {
      try {
        listener(event);
      } catch (error) {
        console.warn('⚠️ Session event listener error:', error);
      }
    });
  }

  /**
   * Register connection identity (called when someone connects)
   */
  registerConnectionIdentity(connectionId: string, identity: ConnectionIdentity): void {
    this.connectionIdentities.set(connectionId, identity);
    console.log(`🆔 Registered connection: ${connectionId} as ${identity.starter}/${identity.identity.name}`);
    
    if (identity.sessionContext) {
      console.log(`📋 Session context: ${identity.sessionContext}`);
    }
  }

  /**
   * Get connection identity
   */
  getConnectionIdentity(connectionId: string): ConnectionIdentity | null {
    return this.connectionIdentities.get(connectionId) || null;
  }

  /**
   * Query available sessions for joining
   */
  queryAvailableSessions(filter?: {
    starter?: string;
    user?: string;
    type?: string;
    active?: boolean;
    joinable?: boolean;
  }): BrowserSession[] {
    return Array.from(this.sessions.values()).filter(session => {
      if (filter?.starter && session.identity.starter !== filter.starter) return false;
      if (filter?.user && session.identity.user !== filter.user) return false;
      if (filter?.type && session.identity.type !== filter.type) return false;
      if (filter?.active !== undefined && session.isActive !== filter.active) return false;
      if (filter?.joinable !== undefined) {
        // Sessions are joinable if they're active and belong to the same user/team
        const isJoinable = session.isActive && session.identity.type === 'collaboration';
        if (isJoinable !== filter.joinable) return false;
      }
      return true;
    });
  }

  /**
   * Create session for identified connection - with option to join existing
   */
  async createSessionForConnection(
    connectionId: string,
    options: {
      autoCleanup?: boolean;
      cleanupAfterMs?: number;
      url?: string;
      joinExisting?: string; // Session ID to join
    } = {}
  ): Promise<string> {
    const identity = this.connectionIdentities.get(connectionId);
    if (!identity) {
      throw new Error(`Connection ${connectionId} not identified. Call registerConnectionIdentity first.`);
    }

    // Join existing session if requested
    if (options.joinExisting) {
      return await this.joinSession(connectionId, options.joinExisting);
    }

    return await this.createSession(identity.identity, {
      ...options,
      starter: identity.starter,
      ...(identity.sessionContext && { sessionContext: identity.sessionContext })
    });
  }

  /**
   * Join an existing session
   */
  async joinSession(connectionId: string, sessionId: string): Promise<string> {
    const session = this.sessions.get(sessionId);
    if (!session) {
      throw new Error(`Session ${sessionId} not found`);
    }

    if (!session.isActive) {
      throw new Error(`Session ${sessionId} is not active`);
    }

    const identity = this.connectionIdentities.get(connectionId);
    if (!identity) {
      throw new Error(`Connection ${connectionId} not identified`);
    }

    // Emit join event
    this.emitEvent({
      type: 'session_joined',
      sessionId,
      identity: identity.identity,
      timestamp: new Date(),
      metadata: {
        joinedBy: identity.identity.name,
        originalOwner: session.identity.name,
        connectionId
      }
    });

    this.updateSessionActivity(sessionId);
    
    console.log(`🤝 ${identity.identity.name} joined session: ${sessionId}`);
    return sessionId;
  }

  /**
   * Find latest session (most recently active)
   */
  getLatestSession(filter?: {
    starter?: string;
    user?: string;
    type?: string;
    active?: boolean;
  }): BrowserSession | null {
    const sessions = this.queryAvailableSessions(filter);
    if (sessions.length === 0) return null;

    return sessions.reduce((latest, session) => 
      session.lastActive > latest.lastActive ? session : latest
    );
  }

  /**
   * Create a new isolated browser session
   */
  async createSession(
    identity: SessionIdentity,
    options: {
      autoCleanup?: boolean;
      cleanupAfterMs?: number;
      url?: string;
      sessionContext?: string;
      starter?: string;
    } = {}
  ): Promise<string> {
    const sessionId = this.generateSessionId(identity, options.sessionContext);
    const type = this.mapIdentityToType(identity);
    const storageDir = await this.createSessionStorage(type, identity.name, sessionId, options.sessionContext);

    const session: BrowserSession = {
      id: sessionId,
      identity,
      type,
      owner: identity.name,
      created: new Date(),
      lastActive: new Date(),
      
      mainBrowser: {
        pid: null,
        url: options.url || 'http://localhost:9000',
        isConnected: false
      },
      
      devTools: {
        pid: null,
        isOpen: false,
        tabs: []
      },
      
      artifacts: {
        storageDir,
        logs: {
          server: [],
          client: []
        },
        screenshots: [],
        files: [],
        recordings: []
      },
      
      isActive: true,
      shouldAutoCleanup: options.autoCleanup ?? true,
      cleanupAfterMs: options.cleanupAfterMs ?? (2 * 60 * 60 * 1000) // 2 hours default
    };

    this.sessions.set(sessionId, session);
    
    // Emit session created event
    this.emitEvent({
      type: 'session_created',
      sessionId,
      identity,
      timestamp: new Date(),
      metadata: {
        starter: options.starter || identity.starter,
        storageDir,
        type
      }
    });
    
    console.log(`📋 Created session: ${sessionId} for ${identity.name} (${type})`);
    console.log(`📁 Storage: ${storageDir}`);
    
    return sessionId;
  }

  /**
   * Launch browser for specific session
   */
  async launchBrowserForSession(
    sessionId: string,
    options: {
      openDevTools?: boolean;
      devToolsTabs?: string[];
      browserOptions?: Record<string, any>;
    } = {}
  ): Promise<{ success: boolean; error?: string }> {
    const session = this.sessions.get(sessionId);
    if (!session) {
      return { success: false, error: `Session ${sessionId} not found` };
    }

    try {
      // Launch main browser
      const browserResult = await this.launchMainBrowser(session, options.browserOptions);
      if (!browserResult.success) {
        return browserResult;
      }

      // Launch DevTools separately if requested
      if (options.openDevTools) {
        const devToolsResult = await this.launchDevTools(session, options.devToolsTabs);
        if (!devToolsResult.success) {
          console.warn(`⚠️ DevTools launch failed for ${sessionId}: ${devToolsResult.error}`);
          // Don't fail the whole operation if DevTools fails
        }
      }

      this.updateSessionActivity(sessionId);
      
      console.log(`🚀 Browser launched for session: ${sessionId}`);
      return { success: true };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { success: false, error: errorMessage };
    }
  }

  /**
   * Launch main browser window for session
   */
  private async launchMainBrowser(
    session: BrowserSession,
    options: Record<string, any> = {}
  ): Promise<{ success: boolean; error?: string }> {
    try {
      const { spawn } = await import('child_process');
      
      // Build browser command with session-specific arguments
      const command = this.buildBrowserCommand(session, options);
      
      const process = spawn(command.cmd, command.args, {
        detached: true,
        stdio: 'ignore',
        cwd: session.artifacts.storageDir // Set working directory to session storage
      });

      process.unref();

      session.mainBrowser.pid = process.pid || null;
      session.lastActive = new Date();

      console.log(`📱 Main browser launched for ${session.id} (PID: ${process.pid})`);
      return { success: true };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { success: false, error: errorMessage };
    }
  }

  /**
   * Launch DevTools as separate window
   */
  private async launchDevTools(
    session: BrowserSession,
    tabs: string[] = ['console']
  ): Promise<{ success: boolean; error?: string }> {
    try {
      // DevTools is launched as a separate process/window
      // Implementation depends on browser type and platform
      
      console.log(`🔧 Opening DevTools for session ${session.id}`);
      console.log(`🔧 Tabs: ${tabs.join(', ')}`);
      
      // For Chrome-based browsers, we can use remote debugging
      if (await this.supportsChromeDevTools()) {
        return await this.launchChromeDevTools(session, tabs);
      }
      
      // Fallback: browser-specific DevTools opening
      return await this.launchGenericDevTools(session, tabs);

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { success: false, error: errorMessage };
    }
  }

  /**
   * Launch Chrome DevTools via remote debugging
   */
  private async launchChromeDevTools(
    session: BrowserSession,
    tabs: string[]
  ): Promise<{ success: boolean; error?: string }> {
    try {
      // Chrome DevTools can be opened via remote debugging protocol
      const debugPort = 9222 + parseInt(session.id.slice(-3)); // Unique port per session
      
      console.log(`🔧 Chrome DevTools on port ${debugPort} for session ${session.id}`);
      
      session.devTools.isOpen = true;
      session.devTools.tabs = tabs;
      
      // TODO: Implement actual Chrome DevTools Protocol connection
      // For now, just mark as opened
      
      return { success: true };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { success: false, error: errorMessage };
    }
  }

  /**
   * Launch generic DevTools (browser-specific)
   */
  private async launchGenericDevTools(
    session: BrowserSession,
    tabs: string[]
  ): Promise<{ success: boolean; error?: string }> {
    console.log(`🔧 Generic DevTools for session ${session.id}`);
    
    session.devTools.isOpen = true;
    session.devTools.tabs = tabs;
    
    // Browser-specific DevTools opening would go here
    // For now, just mark as opened
    
    return { success: true };
  }

  /**
   * Add artifact to session
   */
  async addArtifact(
    sessionId: string,
    artifact: Omit<SessionArtifact, 'timestamp'>
  ): Promise<string> {
    const session = this.sessions.get(sessionId);
    if (!session) {
      throw new Error(`Session ${sessionId} not found`);
    }

    const timestamp = new Date();
    const filename = this.generateArtifactFilename(artifact.type, artifact.source, timestamp);
    const fullPath = path.join(session.artifacts.storageDir, filename);

    // Store artifact reference
    const _fullArtifact: SessionArtifact = {
      ...artifact,
      timestamp,
      path: fullPath
    };

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
    }

    this.updateSessionActivity(sessionId);
    
    console.log(`📎 Added ${artifact.type} artifact to session ${sessionId}: ${filename}`);
    return fullPath;
  }

  /**
   * Get session information
   */
  getSession(sessionId: string): BrowserSession | null {
    return this.sessions.get(sessionId) || null;
  }

  /**
   * List all active sessions
   */
  getActiveSessions(): BrowserSession[] {
    return Array.from(this.sessions.values()).filter(s => s.isActive);
  }

  /**
   * List sessions by type or owner
   */
  getSessionsByFilter(filter: {
    type?: BrowserSession['type'];
    owner?: string;
    active?: boolean;
  }): BrowserSession[] {
    return Array.from(this.sessions.values()).filter(session => {
      if (filter.type && session.type !== filter.type) return false;
      if (filter.owner && session.owner !== filter.owner) return false;
      if (filter.active !== undefined && session.isActive !== filter.active) return false;
      return true;
    });
  }

  /**
   * Close session and cleanup
   */
  async closeSession(sessionId: string, options: { preserveArtifacts?: boolean } = {}): Promise<void> {
    const session = this.sessions.get(sessionId);
    if (!session) {
      console.warn(`⚠️ Session ${sessionId} not found for closure`);
      return;
    }

    console.log(`🔒 Closing session: ${sessionId}`);

    // Close browser processes
    if (session.mainBrowser.pid) {
      try {
        process.kill(session.mainBrowser.pid);
        console.log(`🔴 Closed main browser (PID: ${session.mainBrowser.pid})`);
      } catch (error) {
        console.warn(`⚠️ Failed to close main browser: ${error}`);
      }
    }

    if (session.devTools.pid) {
      try {
        process.kill(session.devTools.pid);
        console.log(`🔴 Closed DevTools (PID: ${session.devTools.pid})`);
      } catch (error) {
        console.warn(`⚠️ Failed to close DevTools: ${error}`);
      }
    }

    // Mark as inactive
    session.isActive = false;

    // Cleanup artifacts if requested
    if (!options.preserveArtifacts) {
      await this.cleanupSessionArtifacts(session);
    }

    this.sessions.delete(sessionId);
    console.log(`✅ Session ${sessionId} closed and cleaned up`);
  }

  /**
   * Map identity to session type
   */
  private mapIdentityToType(identity: SessionIdentity): BrowserSession['type'] {
    switch (identity.type) {
      case 'development': return 'development';
      case 'debugging': return 'development';
      case 'testing': return 'test';
      case 'automation': return 'git-hook';
      case 'collaboration': return 'user';
      default: return 'user';
    }
  }

  /**
   * Create a default session for CLI users (connect to existing or create new)
   */
  async createOrConnectDefaultSession(
    identity: SessionIdentity,
    options: {
      autoCleanup?: boolean;
      cleanupAfterMs?: number;
      url?: string;
    } = {}
  ): Promise<string> {
    // For CLI, try to find existing session first
    if (identity.starter === 'cli') {
      const existing = this.getLatestSession({
        starter: 'cli',
        user: identity.user || identity.name,
        active: true
      });

      if (existing) {
        console.log(`🔌 Connecting to existing CLI session: ${existing.id}`);
        this.updateSessionActivity(existing.id);
        return existing.id;
      }
    }

    // Create new session if none found
    return await this.createSession(identity, { ...options, starter: identity.starter });
  }

  /**
   * Generate meaningful session ID with structure: starter-name-context-timestamp
   */
  private generateSessionId(identity: SessionIdentity, sessionContext?: string): string {
    const today = new Date();
    const dateStr = today.getFullYear().toString().slice(-2) + 
                   String(today.getMonth() + 1).padStart(2, '0') + 
                   String(today.getDate()).padStart(2, '0');
    const timeStr = String(today.getHours()).padStart(2, '0') + 
                   String(today.getMinutes()).padStart(2, '0');
    
    // Build meaningful name: cli-joel-main-dev-250701-1234
    const parts = [
      identity.starter,
      identity.name,
      sessionContext || identity.metadata?.task || identity.metadata?.branch || 'session',
      dateStr,
      timeStr
    ];
    
    return parts.filter(Boolean).join('-').toLowerCase().replace(/[^a-z0-9-]/g, '');
  }

  /**
   * Create session storage directory with proper organization
   */
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
        // Use 'validation' directory for git hooks, with optional branch context
        const branchContext = sessionContext ? `-${sessionContext}` : '';
        sessionPath = path.join(this.artifactRoot, 'validation', `${sessionId}${branchContext}`);
        break;
        
      case 'development':
      case 'test':
        // User sessions go in user directory
        sessionPath = path.join(this.artifactRoot, 'user', owner, sessionId);
        break;
        
      case 'persona':
        // Persona sessions organized by persona name
        sessionPath = path.join(this.artifactRoot, 'personas', owner, sessionId);
        break;
        
      default:
        // Fallback to flat structure
        sessionPath = path.join(this.artifactRoot, 'misc', sessionId);
    }
    
    // Create main session directory
    await fs.mkdir(sessionPath, { recursive: true });
    
    // Create standard subdirectories for artifacts
    const subdirs = ['logs', 'screenshots', 'files', 'recordings', 'devtools'];
    for (const subdir of subdirs) {
      await fs.mkdir(path.join(sessionPath, subdir), { recursive: true });
    }
    
    // Create session metadata file
    const metadata = {
      sessionId,
      type,
      owner,
      sessionContext,
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
    
    console.log(`📁 Created isolated session: ${sessionPath}`);
    return sessionPath;
  }

  /**
   * Generate artifact filename
   */
  private generateArtifactFilename(type: string, source: string, timestamp: Date): string {
    const dateStr = timestamp.toISOString().replace(/[:.]/g, '-');
    const extension = this.getArtifactExtension(type);
    return `${dateStr}-${source}-${type}.${extension}`;
  }

  /**
   * Get file extension for artifact type
   */
  private getArtifactExtension(type: string): string {
    switch (type) {
      case 'log': return 'log';
      case 'screenshot': return 'png';
      case 'recording': return 'webm';
      case 'file': return 'json';
      default: return 'txt';
    }
  }

  /**
   * Update session last active time
   */
  private updateSessionActivity(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.lastActive = new Date();
    }
  }

  /**
   * Check if browser supports Chrome DevTools
   */
  private async supportsChromeDevTools(): Promise<boolean> {
    // Simple check - in real implementation, would detect browser type
    return true; // Most modern browsers support DevTools protocol
  }

  /**
   * Build browser command for session
   */
  private buildBrowserCommand(session: BrowserSession, _options: Record<string, any>): { cmd: string; args: string[] } {
    const platform = process.platform;
    
    // Platform-specific browser launch with session isolation
    if (platform === 'darwin') {
      return {
        cmd: 'open',
        args: [
          '-a', 'Safari', // TODO: Use detected browser
          '--new',
          '--args',
          `--user-data-dir=${session.artifacts.storageDir}`,
          session.mainBrowser.url
        ]
      };
    } else {
      return {
        cmd: 'xdg-open', // Generic fallback
        args: [session.mainBrowser.url]
      };
    }
  }

  /**
   * Start cleanup monitoring for abandoned sessions
   */
  private startCleanupMonitoring(): void {
    this.cleanupInterval = setInterval(async () => {
      const now = Date.now();
      
      for (const [sessionId, session] of this.sessions) {
        if (!session.shouldAutoCleanup || !session.isActive) continue;
        
        const age = now - session.lastActive.getTime();
        if (age > session.cleanupAfterMs) {
          console.log(`🧹 Auto-cleaning abandoned session: ${sessionId} (${Math.round(age / 60000)}min old)`);
          await this.closeSession(sessionId, { preserveArtifacts: false });
        }
      }
    }, 5 * 60 * 1000); // Check every 5 minutes
  }

  /**
   * Cleanup session artifacts
   */
  private async cleanupSessionArtifacts(session: BrowserSession): Promise<void> {
    try {
      await fs.rm(session.artifacts.storageDir, { recursive: true, force: true });
      console.log(`🗑️ Cleaned up artifacts for session ${session.id}`);
    } catch (error) {
      console.warn(`⚠️ Failed to cleanup artifacts for ${session.id}:`, error);
    }
  }

  /**
   * Shutdown session manager
   */
  async shutdown(): Promise<void> {
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
    }

    // Close all active sessions
    const activeSessions = this.getActiveSessions();
    for (const session of activeSessions) {
      await this.closeSession(session.id, { preserveArtifacts: true });
    }

    console.log('🛑 Session manager shutdown complete');
  }
}