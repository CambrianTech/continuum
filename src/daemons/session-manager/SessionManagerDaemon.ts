// ISSUES: 1 open, last updated 2025-07-14 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🚨 ISSUE #1: Dead routing diagnosis code removed - HTTP screenshot requests never reach CommandProcessor despite claiming handle_api
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
 * 
 * ✅ CLEANED UP: Added session.extract handler with proper types (2025-07-13)
 * ✅ CLEANED UP: Modern web standards session extraction (2025-07-13)
 */

import { BaseDaemon } from '../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol';
import { DaemonType } from '../base/DaemonTypes';
import { ContinuumContext, continuumContextFactory } from '../../types/shared/core/ContinuumTypes';
import { SystemEventType, MessageType, SessionCreatedPayload, SessionJoinedPayload } from '../base/EventTypes';
import { SessionConsoleLogger } from './modules/SessionConsoleLogger';
import { SessionRequest } from '../../types/SessionParameters';
import { SessionConnectResponse } from '../../types/SessionConnectResponse';
import { SessionExtractionRequest, SessionExtractionResponse, SessionInfo } from '../../types/shared/SessionTypes';
import { DAEMON_EVENT_BUS } from '../base/DaemonEventBus';
import { MESSAGE_HANDLER_REGISTRY } from '../../integrations/websocket/core/MessageHandlerRegistry';
import { SendToSessionHandler } from './handlers/SendToSessionHandler';
import { RemoteExecutionHandler } from './handlers/RemoteExecutionHandler';
import { DAEMON_REGISTRY } from '../base/DaemonRegistry';
import { DirectoryService } from './services/DirectoryService';
import { CleanupService } from './services/CleanupService';
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

export type SessionType = 'persona' | 'portal' | 'git-hook' | 'development' | 'test';

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
  public readonly daemonType = DaemonType.SESSION_MANAGER;
  
  private sessions = new Map<string, BrowserSession>();
  private connectionIdentities = new Map<string, ConnectionIdentity>();
  private cleanupInterval?: NodeJS.Timeout;
  private eventListeners = new Set<(event: SessionEvent) => void>();
  private consoleLoggers = new Map<string, SessionConsoleLogger>(); // sessionId -> logger
  private remoteExecutionHandler?: RemoteExecutionHandler;
  private directoryService: DirectoryService;
  private cleanupService: CleanupService;
  
  // SEMAPHORE: Prevent race conditions in session creation
  private sessionCreationLock = new Map<string, Promise<BrowserSession>>();

  constructor(context: ContinuumContext, artifactRoot: string = '.continuum/sessions') {
    super(context);
    this.directoryService = new DirectoryService(context, artifactRoot);
    this.cleanupService = new CleanupService(context, this.sessions, this.log.bind(this));
  }

  protected async onStart(): Promise<void> {
    this.log('📋 Starting Session Manager Daemon...');
    this.log('🕵️‍♂️🕵️‍♂️🕵️‍♂️ DISCOVERY: Starting daemon reconnaissance 🕵️‍♂️🕵️‍♂️🕵️‍♂️');
    
    // Test UniversalLogger with all log levels
    const { UniversalLogger } = await import('../../logging/UniversalLogger');
    
    UniversalLogger.log('server', 'SessionManagerDaemon', 'Session Manager Daemon v3.0 started - session management only', 'info', this.context);
    UniversalLogger.log('server', 'SessionManagerDaemon', 'Debug: Initializing directory structure', 'debug', this.context);
    UniversalLogger.log('server', 'SessionManagerDaemon', 'Warning: This is a test warning message', 'warn', this.context);
    UniversalLogger.log('server', 'SessionManagerDaemon', 'Error: This is a test error message', 'error', this.context);
    
    await this.initializeDirectoryStructure();
    
    // Discover all registered daemons after initialization
    setTimeout(() => {
      try {
        const { DAEMON_REGISTRY } = require('../../daemons/base/DaemonRegistry');
        const allDaemons = DAEMON_REGISTRY.getAllDaemons();
        this.log(`🕵️‍♂️ DISCOVERY: Found ${allDaemons.length} total daemons:`);
        allDaemons.forEach((reg: any) => {
          this.log(`🕵️‍♂️   - ${reg.name} (${reg.type}) - Active: ${reg.isActive}`);
          // Check if this daemon handles HTTP
          if (reg.daemon && typeof reg.daemon.getMessageTypes === 'function') {
            const messageTypes = reg.daemon.getMessageTypes();
            this.log(`🕵️‍♂️     Message types: ${messageTypes.join(', ')}`);
            
            // ROUTING DIAGNOSIS: Log which daemons claim to handle routing messages
            const routingTypes = messageTypes.filter((type: string) => 
              type.includes('http') || type.includes('api') || type.includes('route') || type.includes('handle_api')
            );
            this.log(`🔧 ROUTING DIAGNOSIS: ${reg.name} claims routing types: ${routingTypes.join(', ')}`);
          } else {
            this.log(`🕵️‍♂️     No getMessageTypes method - legacy daemon`);
          }
        });
      } catch (error) {
        this.log(`🕵️‍♂️ DISCOVERY ERROR: ${error}`);
      }
    }, 3000);
    // DELEGATED: Cleanup monitoring now handled by CleanupService
    this.log('📅 [DELEGATED] Starting cleanup monitoring via CleanupService');
    this.cleanupService.startCleanupMonitoring();
    
    // Register message handlers for session-related WebSocket messages
    await this.registerMessageHandlers();
    
    // Listen for WebSocket connections with proper typing
    DAEMON_EVENT_BUS.onEvent(SystemEventType.WEBSOCKET_CONNECTION_ESTABLISHED, async (event) => {
      await this.handleWebSocketConnection(event.connectionId, event.metadata);
    });
    
    DAEMON_EVENT_BUS.onEvent(SystemEventType.WEBSOCKET_CONNECTION_CLOSED, async (event) => {
      await this.handleWebSocketDisconnection(event.connectionId, event.reason);
    });
    
    this.log('✅ Session Manager ready for coordination');
  }
  
  /**
   * Register with WebSocket daemon - called by startup system
   */
  async registerWithWebSocketDaemon(webSocketDaemon: any): Promise<void> {
    this.log('📋 Registering session handlers with WebSocket daemon');
    
    try {
      // Create session message handler with access to connection mapping
      const sendToSessionHandler = new SendToSessionHandler(
        webSocketDaemon.getConnectionSessions(),
        webSocketDaemon.sendToConnectionById.bind(webSocketDaemon)
      );
      
      // Create remote execution handler for request-response WebSocket communication
      this.remoteExecutionHandler = new RemoteExecutionHandler(
        webSocketDaemon.getConnectionSessions(),
        webSocketDaemon.sendToConnectionById.bind(webSocketDaemon),
        (correlationId: string, _response: any) => {
          // This will be called when responses arrive
          this.log(`📨 Remote execution response received: ${correlationId}`);
        }
      );
      
      // Register handlers for session-related messages
      MESSAGE_HANDLER_REGISTRY.registerHandler('send_to_session', sendToSessionHandler, this.name);
      MESSAGE_HANDLER_REGISTRY.registerHandler('remote_execution', this.remoteExecutionHandler, this.name);
      
      // Register handler for remote execution responses from browser
      MESSAGE_HANDLER_REGISTRY.registerHandler('remote_execution_response', {
        priority: 100,
        handle: async (data: any) => {
          if (this.remoteExecutionHandler && data.correlationId) {
            this.remoteExecutionHandler.handleResponse(data.correlationId, data);
            return { success: true };
          }
          return { success: false, error: 'No remote execution handler or correlation ID' };
        }
      }, this.name);
      
      this.log('✅ Registered session message handlers with WebSocket daemon');
    } catch (error) {
      this.log(`❌ Failed to register message handlers: ${error}`, 'error');
    }
  }

  /**
   * Register message handlers with WebSocket daemon (legacy)
   */
  private async registerMessageHandlers(): Promise<void> {
    try {
      // Get WebSocket daemon to access connection methods
      const webSocketDaemon = await this.getWebSocketDaemon();
      
      if (webSocketDaemon) {
        // Create session message handler with access to connection mapping
        const sendToSessionHandler = new SendToSessionHandler(
          webSocketDaemon.getConnectionSessions(),
          webSocketDaemon.sendToConnectionById.bind(webSocketDaemon)
        );
        
        // Register handlers for session-related messages
        MESSAGE_HANDLER_REGISTRY.registerHandler('send_to_session', sendToSessionHandler, this.name);
        
        this.log('📋 Registered session message handlers with WebSocket daemon');
      } else {
        this.log('⚠️ WebSocket daemon not found - session handlers not registered');
      }
    } catch (error) {
      this.log(`❌ Failed to register message handlers: ${error}`, 'error');
    }
  }
  
  /**
   * Get WebSocket daemon instance using daemon discovery
   */
  private async getWebSocketDaemon(): Promise<any> {
    try {
      // Debug: List all registered daemons
      const allDaemons = DAEMON_REGISTRY.getAllDaemons();
      this.log(`🔍 Daemon discovery debug - Found ${allDaemons.length} registered daemons:`);
      allDaemons.forEach(reg => {
        this.log(`   - ${reg.name} (${reg.type}) - Active: ${reg.isActive}`);
      });
      
      // Use daemon registry for discovery
      const webSocketDaemon = DAEMON_REGISTRY.findDaemon('websocket-server');
      
      if (webSocketDaemon) {
        this.log('✅ Found WebSocket daemon via registry');
        return webSocketDaemon;
      } else {
        // Try waiting for it to register (in case of startup ordering)
        this.log('⏳ Waiting for WebSocket daemon to register...');
        const daemon = await DAEMON_REGISTRY.waitForDaemon('websocket-server', 3000);
        
        if (daemon) {
          this.log('✅ WebSocket daemon registered after waiting');
          return daemon;
        } else {
          this.log('❌ WebSocket daemon not found after timeout');
          // Debug: Check the old registry too
          const { DAEMON_REGISTRY: OLD_REGISTRY } = await import('../base/BaseDaemon');
          const webSocketDaemonOld = OLD_REGISTRY.get('websocket-server');
          if (webSocketDaemonOld) {
            this.log('✅ Found WebSocket daemon in old registry, using it');
            return webSocketDaemonOld;
          } else {
            this.log('❌ WebSocket daemon not in old registry either');
            this.log(`🔍 Old registry has: ${Array.from(OLD_REGISTRY.keys()).join(', ')}`);
          }
          return null;
        }
      }
    } catch (error) {
      this.log(`❌ Error finding WebSocket daemon: ${error}`, 'error');
      return null;
    }
  }

  /**
   * Get message types this daemon handles
   */
  getMessageTypes(): string[] {
    return [
      'create_session',
      'create_session_for_connection', 
      'get_session',
      'get_session_info',
      'list_sessions',
      'add_artifact',
      'close_session',
      'register_connection_identity',
      'session.connect',
      'session.extract'
    ];
  }

  protected async onStop(): Promise<void> {
    this.log('🛑 Stopping Session Manager...');
    
    // DELEGATED: Cleanup monitoring now handled by CleanupService
    this.log('🛑 [DELEGATED] Stopping cleanup monitoring via CleanupService');
    this.cleanupService.stopCleanupMonitoring();
    
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
    }
    
    // Stop all console loggers
    for (const [sessionId, logger] of Array.from(this.consoleLoggers.entries())) {
      try {
        await logger.stopLogging();
        this.log(`🔌 Stopped console logging for session ${sessionId}`);
      } catch (error) {
        this.log(`⚠️ Error stopping console logger for ${sessionId}: ${error}`, 'warn');
      }
    }
    this.consoleLoggers.clear();
    
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

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse<any>> {
    this.log(`🎮🎮🎮 SESSION MANAGER: Received message type: ${message.type} 🎮🎮🎮`);
    this.log(`🎮🎮🎮 SESSION MANAGER: Call stack: ${new Error().stack?.split('\n').slice(1, 4).join(' -> ')}`);
    
    // TODO: REMOVE - Dead routing diagnosis code (confirmed CommandProcessor claims handle_api but never receives screenshot requests)
    // if (message.type === 'session.extract') {
    //   // Dead code - routing diagnosis spam removed 
    // }
    
    if (message.type.includes('api') || message.type.includes('http') || message.type.includes('command')) {
      this.log(`🎮🎮🎮 SESSION MANAGER: HTTP/API/COMMAND message details: ${JSON.stringify(message, null, 2)}`);
    }
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
          
        case 'get_session_info':
          return this.handleGetSessionInfo(message.data);
          
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
          
        case 'start_console_logging':
          return await this.handleStartConsoleLogging(message.data);
          
        case 'stop_console_logging':
          return await this.handleStopConsoleLogging(message.data);
          
        case 'enable_server_logging':
          return await this.handleEnableServerLogging(message.data);
          
        case 'session.connect':
          return await this.handleConnect(message.data as SessionRequest);
          
        case 'session.extract':
          return await this.handleSessionExtract(message.data as SessionExtractionRequest);
          
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
   * Handle intelligent connection orchestration
   */
  async handleConnect(connectParams: SessionRequest): Promise<DaemonResponse<SessionConnectResponse>> {
    // Debug: Log the actual parameters received
    this.log(`🔍 handleConnect called with: ${JSON.stringify(connectParams)}`, 'info');
    this.log(`🔍 FOCUS_DEBUG: focus=${connectParams.focus}, killZombies=${connectParams.killZombies}`, 'info');
    
    const { source, owner = 'shared', sessionPreference = 'current', capabilities = [], context = 'development', sessionType = 'development', focus = false, killZombies = false } = connectParams;
    
    this.log(`🔍 After destructuring: owner=${owner}, sessionPreference=${sessionPreference}, sessionType=${sessionType}, focus=${focus}, killZombies=${killZombies}`, 'info');

    try {
      let session: BrowserSession | null = null;
      let action: 'joined_existing' | 'created_new' | 'forked_from' = 'joined_existing';
      let newLogFiles = false;

      // Intelligent session routing
      if (sessionPreference === 'new' || sessionPreference.startsWith('fork:')) {
        // Force new session or fork from existing
        if (sessionPreference.startsWith('fork:')) {
          const forkFromId = sessionPreference.split(':')[1];
          session = await this.forkSession(forkFromId, { owner, type: sessionType as BrowserSession['type'], context });
          action = 'forked_from';
        } else {
          session = await this.createOrFindSession(
            owner, // connectionId for new sessions
            sessionType as BrowserSession['type'],
            { userAgent: source || 'handleConnect', url: context || 'new-session' }
          );
          action = 'created_new';
        }
        newLogFiles = true;
      } else if (sessionPreference !== 'current' && sessionPreference.length > 10) {
        // Specific session ID requested
        session = this.getSession(sessionPreference);
        if (!session) {
          return { success: false, error: `Session ${sessionPreference} not found` };
        }
      } else {
        // Default: find or create SHARED session (not owner-specific)
        // Look for the shared development session that everyone uses
        session = this.getLatestSession({
          type: sessionType as BrowserSession['type'],
          active: true
          // Note: removed owner filter to find shared session
        });

        if (!session) {
          // Create the shared session using createOrFindSession for consistency
          session = await this.createOrFindSession(
            'shared-session', // connectionId for shared sessions
            sessionType as BrowserSession['type'],
            { userAgent: source || 'handleConnect', url: context || 'development' }
          );
          action = 'created_new';
          newLogFiles = true;
          this.log(`✨ Created new shared development session: ${session.id}`);
        } else {
          this.log(`🔄 Joining existing shared development session: ${session.id}`);
        }
      }

      if (!session) {
        return { success: false, error: 'Failed to get or create session' };
      }
      
      // Automatically enable server logging for ALL sessions
      if (action === 'created_new' || action === 'forked_from') {
        try {
          const serverLogPath = session.artifacts.logs.server[0];
          
          // Configure THIS daemon to log to the session file
          this.setSessionLogPath(serverLogPath);
          
          // Emit session_created event on global bus for other daemons
          DAEMON_EVENT_BUS.emitEvent(SystemEventType.SESSION_CREATED, { 
            sessionId: session.id, 
            serverLogPath,
            sessionType: session.type,
            owner: session.owner,
            focus,
            killZombies
          });
          
          this.log(`🔌 Enabled server logging for session ${session.id}: ${serverLogPath}`);
        } catch (error) {
          this.log(`⚠️ Failed to enable server logging: ${error}`, 'warn');
        }
      } else if (action === 'joined_existing') {
        // Emit session_joined event on global bus for existing sessions to ensure browser management
        const sessionJoinedEvent: SessionJoinedPayload = {
          sessionId: session.id, 
          sessionType: session.type,
          owner: session.owner,
          source: source || 'unknown',
          focus,
          killZombies
        };
        DAEMON_EVENT_BUS.emitEvent(SystemEventType.SESSION_JOINED, sessionJoinedEvent);
        
        this.log(`📋 REFRESH_DEBUG: Emitted session_joined event for ${session.id} (${session.type})`);
      }

      // Orchestrate what needs to be launched based on capabilities
      const launched = {
        browser: capabilities.includes('browser'),
        webserver: true, // Always running
        newLogFiles
      };

      // Get version from package.json
      let version = 'unknown';
      try {
        const packageJson = await import('../../../package.json');
        version = packageJson.version || 'unknown';
      } catch (error) {
        // Fallback to unknown
      }
      
      // Construct strongly typed response
      const response: SessionConnectResponse = {
        version,
        sessionId: session.id,
        action,
        launched,
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
      
      return {
        success: true,
        data: response
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { success: false, error: `Connection orchestration failed: ${errorMessage}` };
    }
  }

  /**
   * Handle session extraction from HTTP headers using modern web standards
   */
  private async handleSessionExtract(data: SessionExtractionRequest): Promise<DaemonResponse<SessionExtractionResponse>> {
    try {
      // Extract session using modern web standards
      const sessionInfo = this.extractSessionFromHeaders(data.headers);
      
      // If session ID found, validate it exists
      if (sessionInfo.sessionId) {
        const sessionExists = this.sessions.has(sessionInfo.sessionId);
        if (!sessionExists) {
          // Session ID provided but doesn't exist - might be expired
          return {
            success: false,
            error: `Session not found: ${sessionInfo.sessionId}`,
            data: { 
              sessionId: sessionInfo.sessionId,
              extractedFrom: sessionInfo.source,
              sessionExists: false
            }
          };
        }
      }
      
      const response: SessionExtractionResponse = {
        ...(sessionInfo.sessionId && { sessionId: sessionInfo.sessionId }),
        extractedFrom: sessionInfo.source,
        sessionExists: sessionInfo.sessionId ? this.sessions.has(sessionInfo.sessionId) : false
      };
      
      return {
        success: true,
        data: response
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Session extraction failed: ${errorMessage}`
      };
    }
  }

  /**
   * Extract session information from HTTP headers using modern web standards
   */
  private extractSessionFromHeaders(headers: Record<string, string>): SessionInfo {
    // 1. Check for X-Session-ID header (modern API standard)
    const sessionHeader = headers['x-session-id'] || headers['X-Session-ID'];
    if (sessionHeader) {
      return {
        sessionId: sessionHeader,
        source: 'header'
      };
    }
    
    // 2. Check for Authorization Bearer token (OAuth2/JWT standard)
    const authHeader = headers['authorization'] || headers['Authorization'];
    if (authHeader?.startsWith('Bearer ')) {
      const token = authHeader.substring(7);
      return {
        sessionId: token,
        source: 'bearer'
      };
    }
    
    // 3. Check for session cookie (modern web standard)
    const cookieHeader = headers['cookie'] || headers['Cookie'];
    if (cookieHeader) {
      // Check for continuum_session_id cookie (browser client standard)
      const continuumSessionMatch = cookieHeader.match(/continuum_session_id=([^;]+)/);
      if (continuumSessionMatch) {
        return {
          sessionId: decodeURIComponent(continuumSessionMatch[1]),
          source: 'cookie'
        };
      }
      
      // Fallback: check for sessionId cookie
      const sessionMatch = cookieHeader.match(/sessionId=([^;]+)/);
      if (sessionMatch) {
        return {
          sessionId: sessionMatch[1],
          source: 'cookie'
        };
      }
    }
    
    return { source: 'none' };
  }

  /**
   * Fork session from existing session
   */
  private async forkSession(fromSessionId: string, options: { owner: string; type: SessionType; context?: string }): Promise<BrowserSession> {
    const sourceSession = this.getSession(fromSessionId);
    if (!sourceSession) {
      throw new Error(`Source session ${fromSessionId} not found for forking`);
    }

    // Create new session with copied context using semaphore protection
    const newSession = await this.createSession({
      type: options.type,
      owner: options.owner,
      context: `forked-from-${fromSessionId}`,
      starter: 'fork-operation',
      identity: { name: options.owner, user: options.owner }
    });

    // TODO: Copy relevant artifacts/state from source session
    this.log(`🍴 Forked session ${newSession.id} from ${fromSessionId}`, 'info');

    return newSession;
  }

  /**
   * Get session by ID
   */
  getSession(sessionId: string): BrowserSession | null {
    return this.sessions.get(sessionId) || null;
  }

  /**
   * Get latest session matching criteria
   */
  getLatestSession(criteria: {
    owner?: string;
    type?: SessionType;
    active?: boolean;
  }): BrowserSession | null {
    const sessions = Array.from(this.sessions.values())
      .filter(session => {
        if (criteria.owner && session.owner !== criteria.owner) return false;
        if (criteria.type && session.type !== criteria.type) return false;
        if (criteria.active !== undefined && session.isActive !== criteria.active) return false;
        return true;
      })
      .sort((a, b) => b.lastActive.getTime() - a.lastActive.getTime());
    
    return sessions.length > 0 ? sessions[0] : null;
  }

  /**
   * Get connections for a specific session
   */
  getSessionConnections(sessionId: string): any[] {
    return Array.from(this.connectionIdentities.values())
      .filter((conn: any) => conn.sessionId === sessionId);
  }

  /**
   * Stop a session and cleanup
   */
  async stopSession(sessionId: string, options: {
    force?: boolean;
    saveArtifacts?: boolean;
    reason?: string;
  } = {}): Promise<{ success: boolean; error?: string }> {
    try {
      const session = this.sessions.get(sessionId);
      if (!session) {
        return { success: false, error: `Session ${sessionId} not found` };
      }

      // Mark session as inactive
      session.isActive = false;
      session.lastActive = new Date();

      // Close connections if force stop
      if (options.force) {
        const connections = this.getSessionConnections(sessionId);
        for (const conn of connections) {
          this.connectionIdentities.delete(conn.connectionId);
        }
      }

      // Save artifacts if requested
      if (options.saveArtifacts) {
        // TODO: Implement artifact saving logic
        this.log(`💾 Artifacts saved for session ${sessionId}`, 'info');
      }

      this.log(`🛑 Session ${sessionId} stopped. Reason: ${options.reason || 'Manual'}`, 'info');
      
      return { success: true };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to stop session ${sessionId}: ${errorMessage}`, 'error');
      return { success: false, error: errorMessage };
    }
  }

  /**
   * Emit session events for other systems to observe
   */
  emitSessionEvent(event: {
    type: 'session_created' | 'session_joined' | 'session_stopped' | 'session_forked';
    sessionId: string;
    timestamp: Date;
    metadata?: any;
  }): void {
    // Emit to event system for other daemons/commands to observe
    this.log(`📡 Session event: ${event.type} for ${event.sessionId}`, 'info');
    
    // TODO: Integrate with proper event system when available
    // EventBus.emit('session', event);
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
      const session = await this.createSession({ type, owner, ...options });
      
      return {
        success: true,
        data: {
          sessionId: session.id,
          session,
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
      const session = await this.createSession({
        type: identity.type as SessionType,
        owner: identity.name,
        ...options,
        sessionContext: identity.sessionContext
      });
      
      return {
        success: true,
        data: {
          sessionId: session.id,
          session,
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
   * Handle get session info request (for console logging integration)
   */
  private handleGetSessionInfo(data: any): DaemonResponse {
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

    // Return session artifacts and key info for browser integration
    return {
      success: true,
      data: {
        sessionId: session.id,
        type: session.type,
        owner: session.owner,
        isActive: session.isActive,
        artifacts: session.artifacts,
        storageDir: session.artifacts.storageDir,
        created: session.created,
        lastActive: session.lastActive
      }
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
        connections: this.connectionIdentities.size,
        consoleLoggers: this.consoleLoggers.size
      }
    };
  }

  /**
   * Handle start console logging request
   */
  private async handleStartConsoleLogging(data: any): Promise<DaemonResponse> {
    const { sessionId, debugUrl, targetId } = data;
    
    if (!sessionId || !debugUrl) {
      return {
        success: false,
        error: 'sessionId and debugUrl are required'
      };
    }

    try {
      const session = this.sessions.get(sessionId);
      if (!session) {
        return {
          success: false,
          error: `Session ${sessionId} not found`
        };
      }

      // Check if console logging is already active for this session
      if (this.consoleLoggers.has(sessionId)) {
        return {
          success: true,
          data: {
            message: `Console logging already active for session ${sessionId}`,
            sessionId,
            debugUrl
          }
        };
      }

      // Create and configure console logger
      const logger = new SessionConsoleLogger();

      // Start logging (UniversalLogger will handle session-specific file writing)
      await logger.startLogging(debugUrl, targetId);
      this.consoleLoggers.set(sessionId, logger);

      this.log(`🔌 Started console logging for session ${sessionId}: ${debugUrl}`);

      return {
        success: true,
        data: {
          sessionId,
          debugUrl,
          message: 'Console logging started successfully'
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to start console logging for ${sessionId}: ${errorMessage}`, 'error');
      return {
        success: false,
        error: `Failed to start console logging: ${errorMessage}`
      };
    }
  }

  /**
   * Handle stop console logging request
   */
  private async handleStopConsoleLogging(data: any): Promise<DaemonResponse> {
    const { sessionId } = data;
    
    if (!sessionId) {
      return {
        success: false,
        error: 'sessionId is required'
      };
    }

    try {
      const logger = this.consoleLoggers.get(sessionId);
      if (!logger) {
        return {
          success: true,
          data: {
            message: `No console logging active for session ${sessionId}`,
            sessionId
          }
        };
      }

      await logger.stopLogging();
      this.consoleLoggers.delete(sessionId);

      this.log(`🔌 Stopped console logging for session ${sessionId}`);

      return {
        success: true,
        data: {
          sessionId,
          message: 'Console logging stopped successfully'
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to stop console logging for ${sessionId}: ${errorMessage}`, 'error');
      return {
        success: false,
        error: `Failed to stop console logging: ${errorMessage}`
      };
    }
  }

  /**
   * Handle enable server logging request - configures all daemons to write to session server log
   */
  private async handleEnableServerLogging(data: any): Promise<DaemonResponse> {
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

    try {
      const serverLogPath = session.artifacts.logs.server[0];
      
      // Configure THIS daemon to log to the session file
      this.setSessionLogPath(serverLogPath);
      
      // Send message to all other daemons to enable session logging
      const daemonTypes: DaemonType[] = [
        DaemonType.WEBSOCKET_SERVER,
        DaemonType.RENDERER, 
        DaemonType.COMMAND_PROCESSOR,
        DaemonType.BROWSER_MANAGER,
        DaemonType.CONTINUUM_DIRECTORY,
        DaemonType.STATIC_FILE
      ];
      const enableResults = [];
      
      for (const daemonType of daemonTypes) {
        try {
          const result = await this.sendMessage(daemonType, MessageType.SET_SESSION_LOG, {
            sessionId,
            logPath: serverLogPath
          });
          enableResults.push({ daemon: daemonType, success: result.success });
        } catch (error) {
          enableResults.push({ daemon: daemonType, success: false, error: error instanceof Error ? error.message : String(error) });
        }
      }
      
      this.log(`🔌 Enabled server logging for session ${sessionId}: ${serverLogPath}`);
      
      return {
        success: true,
        data: {
          sessionId,
          serverLogPath,
          enabledDaemons: enableResults.filter(r => r.success).map(r => r.daemon),
          failedDaemons: enableResults.filter(r => !r.success)
        }
      };
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Failed to enable server logging: ${errorMessage}`
      };
    }
  }

  // Core session management methods (simplified versions of the previous SessionManager)
  
  private async createSession(options: {
    type: BrowserSession['type'];
    owner: string;
    context?: string;
    starter?: string;
    identity?: { name: string; user: string };
    autoCleanup?: boolean;
    cleanupAfterMs?: number;
    url?: string;
    sessionContext?: string;
  }): Promise<BrowserSession> {
    const { type, owner } = options;
    const sessionContext = this.generateSessionContext(type, owner, options.sessionContext);
    const storageDir = await this.createSessionStorage(type, owner, sessionContext.sessionId, options.sessionContext);

    const session: BrowserSession = {
      id: sessionContext.sessionId,
      type,
      owner,
      created: new Date(),
      lastActive: new Date(),
      
      processes: {},
      
      artifacts: {
        storageDir,
        logs: { 
          server: [`${storageDir}/logs/server.log`], 
          client: [`${storageDir}/logs/browser.log`]
        },
        screenshots: [],
        files: [],
        recordings: [],
        devtools: []
      },
      
      isActive: true,
      shouldAutoCleanup: options.autoCleanup ?? true,
      cleanupAfterMs: options.cleanupAfterMs ?? (2 * 60 * 60 * 1000) // 2 hours default
    };

    this.sessions.set(sessionContext.sessionId, session);
    this.log(`📋 Created session: ${sessionContext.sessionId} for ${owner} (${type})`);
    
    // Emit session created event with full session info (local event)
    this.emitEvent({
      type: 'session_created',
      sessionId: sessionContext.sessionId,
      session,
      timestamp: new Date(),
      metadata: {
        storageDir: session.artifacts.storageDir,
        sessionContext: options.sessionContext,
        autoCleanup: session.shouldAutoCleanup
      }
    });

    // CORE EVENT: Emit to global daemon event bus for ALL session creation (any client)
    const sessionCreatedEvent: SessionCreatedPayload = {
      sessionId: session.id, 
      sessionType: session.type,
      owner: session.owner,
      serverLogPath: session.artifacts.logs.server[0] || ''
    };
    DAEMON_EVENT_BUS.emitEvent(SystemEventType.SESSION_CREATED, sessionCreatedEvent);
    this.log(`🌐 REFRESH_DEBUG: Emitted core SESSION_CREATED event for ${sessionContext.sessionId} (${type})`);
    
    return session;
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

    // Stop console logging if active
    const logger = this.consoleLoggers.get(sessionId);
    if (logger) {
      try {
        await logger.stopLogging();
        this.consoleLoggers.delete(sessionId);
        this.log(`🔌 Stopped console logging for session ${sessionId}`);
      } catch (error) {
        this.log(`⚠️ Error stopping console logger: ${error}`, 'warn');
      }
    }

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

    // DELEGATED: Cleanup artifacts now handled by CleanupService
    if (!options.preserveArtifacts) {
      this.log(`🧹 [DELEGATED] Cleaning up artifacts via CleanupService`);
      await this.cleanupService.cleanupSessionArtifacts(session);
    }

    this.sessions.delete(sessionId);
    this.log(`✅ Session ${sessionId} closed and cleaned up`);
  }

  // Helper methods
  
  private async initializeDirectoryStructure(): Promise<void> {
    try {
      // DELEGATED: Directory initialization now handled by DirectoryService
      this.log('📁 [DELEGATED] Initializing directory structure via DirectoryService');
      await this.directoryService.initializeDirectoryStructure();
      this.log('✅ [DELEGATED] Session directory structure initialized');
    } catch (error) {
      this.log(`❌ Failed to initialize session directories: ${error}`, 'error');
    }
  }

  private generateSessionContext(type: string, owner: string, context?: string): ContinuumContext {
    // Create a proper ContinuumContext with UUID-based sessionId
    return continuumContextFactory.create({
      sessionType: type as 'development' | 'production' | 'test',
      sessionOwner: owner,
      environment: 'server',
      ...(context && { sessionContext: context })
    });
  }

  private async createSessionStorage(
    type: BrowserSession['type'], 
    owner: string, 
    sessionId: string,
    sessionContext?: string
  ): Promise<string> {
    // DELEGATED: Directory creation now handled by DirectoryService
    this.log(`📁 [DELEGATED] Creating session storage via DirectoryService`, 'info');
    
    const artifacts = await this.directoryService.createSessionStorage(
      sessionId, type, owner, sessionContext
    );
    
    // Create session metadata for backward compatibility
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
      path.join(artifacts.storageDir, 'session-info.json'), 
      JSON.stringify(metadata, null, 2)
    );
    
    this.log(`✅ [DELEGATED] Session storage created: ${artifacts.storageDir}`, 'info');
    
    return artifacts.storageDir;
  }

  private generateArtifactFilename(type: string, source: string, timestamp: Date): string {
    const dateStr = timestamp.toISOString().replace(/[:.]/g, '-');
    const extension = type === 'log' ? 'log' : type === 'screenshot' ? 'png' : 'json';
    return `${dateStr}-${source}-${type}.${extension}`;
  }

  // REMOVED: Cleanup methods are now delegated to CleanupService
  // - startCleanupMonitoring() -> cleanupService.startCleanupMonitoring()
  // - cleanupSessionArtifacts() -> cleanupService.cleanupSessionArtifacts()

  /**
   * Handle new WebSocket connection from WebSocketDaemon event
   */
  private async handleWebSocketConnection(
    connectionId: string, 
    metadata: { userAgent: string; url: string; headers: Record<string, string> }
  ): Promise<void> {
    try {
      this.log(`🔌 Handling WebSocket connection: ${connectionId}`);
      this.log(`🚨🚨🚨 SCREENSHOT DEBUG VARIANT-B: About to handle WebSocket connection ${connectionId} 🚨🚨🚨`);
      
      // Determine session type based on connection metadata
      const sessionType = this.determineSessionType(metadata);
      
      // Create or find appropriate session
      const session = await this.createOrFindSession(connectionId, sessionType, metadata);
      
      // RACE CONDITION FIX: Check if connection still exists before sending session_ready
      const connectionCheck = await this.sendMessage(DaemonType.WEBSOCKET_SERVER, 'check_connection', {
        connectionId
      });
      
      if (!connectionCheck.success) {
        this.log(`⚠️ Connection ${connectionId} no longer exists, skipping session_ready (likely client disconnected during session setup)`);
        return;
      }
      
      // Send session_ready message to the connection via WebSocketDaemon
      const response = await this.sendMessage(DaemonType.WEBSOCKET_SERVER, MessageType.SEND_TO_CONNECTION, {
        connectionId,
        message: {
          type: 'session_ready',
          data: {
            sessionId: session.id,
            sessionType: session.type,
            devtools: session.type === 'portal' || session.type === 'development',
            logPaths: session.artifacts.logs,
            directories: {
              storage: session.artifacts.storageDir,
              logs: path.join(session.artifacts.storageDir, 'logs')
            }
          },
          timestamp: new Date().toISOString()
        }
      });
      
      if (response.success) {
        this.log(`✅ Sent session_ready to connection ${connectionId} for session ${session.id}`);
      } else {
        this.log(`❌ Failed to send session_ready: ${response.error}`, 'error');
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ Failed to handle WebSocket connection: ${errorMessage}`, 'error');
    }
  }

  /**
   * Handle WebSocket disconnection
   */
  private async handleWebSocketDisconnection(connectionId: string, reason?: string): Promise<void> {
    this.log(`🔌 WebSocket disconnected: ${connectionId}${reason ? ` (${reason})` : ''}`);
    // Sessions persist beyond connections, so we just log for now
    // Could implement connection tracking per session if needed
  }

  /**
   * Determine session type from connection metadata
   */
  private determineSessionType(metadata: { userAgent: string; url: string }): BrowserSession['type'] {
    const { userAgent, url } = metadata;
    
    if (userAgent.includes('git') || url.includes('hook')) {
      return 'git-hook';
    }
    if (userAgent.includes('Portal') || url.includes('portal')) {
      return 'portal';
    }
    if (url.includes('test') || userAgent.includes('test')) {
      return 'test';
    }
    
    return 'development'; // Default for regular browser connections
  }

  /**
   * Create or find an appropriate session for the connection
   */
  private async createOrFindSession(
    connectionId: string,
    sessionType: BrowserSession['type'],
    metadata: { userAgent: string; url: string }
  ): Promise<BrowserSession> {
    
    // SEMAPHORE: Prevent race conditions
    const owner = sessionType === 'development' ? 'shared' : connectionId;
    const lockKey = `${sessionType}-${owner}`;
    
    if (this.sessionCreationLock.has(lockKey)) {
      this.log(`⏳ Session creation in progress for ${lockKey}, waiting...`);
      return await this.sessionCreationLock.get(lockKey)!;
    }

    // Look for existing sessions based on type and owner
    const existingSessions = Array.from(this.sessions.values())
      .filter(s => s.type === sessionType && s.isActive && s.owner === owner);
    
    if (existingSessions.length > 0) {
      const session = existingSessions[0];
      session.lastActive = new Date();
      this.log(`♻️ Reusing existing ${sessionType} session: ${session.id}`);
      return session;
    }
    
    // Create new session with semaphore protection
    const sessionPromise = this.createSession({
      type: sessionType,
      owner: owner,
      starter: metadata.userAgent,
      autoCleanup: sessionType !== 'development',
      context: 'websocket-connection'
    });
    
    this.sessionCreationLock.set(lockKey, sessionPromise);
    
    try {
      const session = await sessionPromise;
      this.log(`✨ Created session with semaphore: ${session.id}`);
      return session;
    } finally {
      this.sessionCreationLock.delete(lockKey);
    }
  }
}