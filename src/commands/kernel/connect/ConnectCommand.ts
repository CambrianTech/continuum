/**
 * Connect Command - Session & Window Management with Daemon Integration
 * 
 * Orchestrates collaborative sessions with intelligent window/tab management
 * Integrates with SessionManagerDaemon, ContinuumDirectoryDaemon, BrowserManagerDaemon
 * Supports Portal-AI collaboration with shared browser state
 */

import { DirectCommand } from '../../core/direct-command/DirectCommand.js';
import { CommandDefinition, CommandContext, CommandResult } from '../../core/base-command/BaseCommand.js';
import * as path from 'path';

// Strongly typed enums for session and window management
export enum SessionType {
  PORTAL = 'portal',
  DEVTOOLS = 'devtools', 
  COLLABORATIVE = 'collaborative',
  GIT_HOOK = 'git-hook',
  INTERACTIVE = 'interactive',
  PERSONA = 'persona'
}

export enum WindowStrategy {
  NEW_WINDOW = 'new-window',
  NEW_TAB = 'new-tab',
  JOIN_EXISTING = 'join-existing',
  SHARED_TAB = 'shared-tab'
}

export enum CollaborationMode {
  SHARED_VIEW = 'shared-view',        // Same tab, synchronized view
  PARALLEL_VIEW = 'parallel-view',    // Parallel tabs in same window
  ISOLATED = 'isolated'               // Separate windows
}

export enum ParticipantType {
  HUMAN_PORTAL = 'human-portal',
  HUMAN_BROWSER = 'human-browser',
  AI_PORTAL = 'ai-portal',
  AI_PERSONA = 'ai-persona',
  CLI_USER = 'cli-user'
}

interface ConnectParams {
  sessionType?: SessionType;
  windowStrategy?: WindowStrategy;
  collaborationMode?: CollaborationMode;
  participantType?: ParticipantType;
  joinSession?: string;              // Existing session ID to join
  maintainWindow?: boolean;          // Keep window open across commands
  devtools?: boolean;
  background?: boolean;
  owner?: string;
}

interface SessionInfo {
  sessionId: string;
  sessionType: SessionType;
  windowStrategy: WindowStrategy;
  collaborationMode: CollaborationMode;
  participants: ParticipantInfo[];
  windowInfo: WindowInfo;
  artifactLocation: string;
}

interface ParticipantInfo {
  id: string;
  type: ParticipantType;
  joinedAt: number;
  capabilities: string[];
}

interface WindowInfo {
  windowId: string;
  tabId: string;
  url: string;
  title: string;
  devToolsAccess: boolean;
}

export class ConnectCommand extends DirectCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'connect',
      category: 'kernel',
      icon: '🔌',
      description: 'Create collaborative sessions with intelligent window/tab management',
      parameters: { 
        sessionType: `Enum: ${Object.values(SessionType).join(' | ')}`,
        windowStrategy: `Enum: ${Object.values(WindowStrategy).join(' | ')}`,
        collaborationMode: `Enum: ${Object.values(CollaborationMode).join(' | ')}`,
        participantType: `Enum: ${Object.values(ParticipantType).join(' | ')}`,
        joinSession: 'string',
        maintainWindow: 'boolean',
        devtools: 'boolean',
        background: 'boolean',
        owner: 'string'
      },
      examples: [
        { 
          description: 'Start Portal-AI collaboration', 
          command: `{"sessionType": "${SessionType.COLLABORATIVE}", "participantType": "${ParticipantType.HUMAN_PORTAL}", "collaborationMode": "${CollaborationMode.SHARED_VIEW}"}` 
        },
        { 
          description: 'Join existing session', 
          command: `{"joinSession": "portal-collab-1905", "windowStrategy": "${WindowStrategy.JOIN_EXISTING}"}` 
        },
        { 
          description: 'DevTools session', 
          command: `{"sessionType": "${SessionType.DEVTOOLS}", "devtools": true, "windowStrategy": "${WindowStrategy.NEW_WINDOW}"}` 
        },
        {
          description: 'AI Persona joins collaboration',
          command: `{"joinSession": "collaborative-1905", "participantType": "${ParticipantType.AI_PERSONA}", "collaborationMode": "${CollaborationMode.SHARED_VIEW}"}`
        }
      ],
      usage: 'Create or join sessions with daemon-coordinated window management and artifact organization'
    };
  }

  protected static async executeOperation(params: any = {}, context?: CommandContext): Promise<CommandResult> {
    const connectParams = this.parseParams<ConnectParams>(params);
    
    try {
      // Use strongly typed enums with defaults
      const sessionType = connectParams.sessionType || SessionType.INTERACTIVE;
      const windowStrategy = connectParams.windowStrategy || WindowStrategy.NEW_WINDOW;
      const collaborationMode = connectParams.collaborationMode || CollaborationMode.ISOLATED;
      const participantType = connectParams.participantType || ParticipantType.CLI_USER;

      let sessionInfo: SessionInfo;

      if (connectParams.joinSession) {
        // Join existing session - delegate to SessionManagerDaemon
        sessionInfo = await this.delegateToSessionManager('join_session', {
          sessionId: connectParams.joinSession,
          participantType,
          windowStrategy,
          collaborationMode
        });
      } else {
        // Create new session - delegate to SessionManagerDaemon  
        sessionInfo = await this.delegateToSessionManager('create_session', {
          sessionType,
          participantType,
          owner: connectParams.owner || 'default'
        });
      }

      // Window management - delegate to WindowManagerDaemon
      const windowInfo = await this.delegateToWindowManager('manage_window', {
        sessionId: sessionInfo.sessionId,
        windowStrategy,
        collaborationMode,
        devtools: connectParams.devtools || false,
        background: connectParams.background || false
      });

      // Update session with window info
      sessionInfo.windowInfo = windowInfo;

      return this.createSuccessResult(
        `Connected to ${sessionType} session ${sessionInfo.sessionId}`,
        {
          session: sessionInfo,
          // Command chaining: Return session info for other commands to use
          sessionContext: {
            sessionId: sessionInfo.sessionId,
            artifactLocation: sessionInfo.artifactLocation,
            windowInfo: sessionInfo.windowInfo,
            collaborationMode: sessionInfo.collaborationMode
          }
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Connection failed: ${errorMessage}`);
    }
  }

  /**
   * Delegate session operations to SessionManagerDaemon
   * Commands don't manage sessions directly - they delegate to specialized daemons
   */
  private static async delegateToSessionManager(operation: string, params: any): Promise<SessionInfo> {
    // TODO: Implement actual daemon communication via WebSocket/message passing
    // This demonstrates the proper delegation pattern:
    
    const daemonMessage = {
      type: 'daemon_request',
      target: 'session-manager',
      operation,
      params,
      requestId: `connect_${Date.now()}`
    };
    
    // In real implementation, this would:
    // 1. Send message via WebSocket to SessionManagerDaemon
    // 2. Wait for response with session info
    // 3. Return strongly typed SessionInfo
    
    // Mock response for now
    const mockSessionInfo: SessionInfo = {
      sessionId: `${params.sessionType || 'collaborative'}-${Date.now()}`,
      sessionType: params.sessionType || SessionType.COLLABORATIVE,
      windowStrategy: params.windowStrategy || WindowStrategy.NEW_WINDOW,
      collaborationMode: params.collaborationMode || CollaborationMode.SHARED_VIEW,
      participants: [{
        id: `participant_${Date.now()}`,
        type: params.participantType || ParticipantType.CLI_USER,
        joinedAt: Date.now(),
        capabilities: ['screenshot', 'navigate', 'inspect']
      }],
      windowInfo: {
        windowId: '',
        tabId: '',
        url: '',
        title: '',
        devToolsAccess: false
      },
      artifactLocation: `.continuum/sessions/${params.sessionType || 'collaborative'}-${Date.now()}/artifacts/`
    };
    
    return mockSessionInfo;
  }

  /**
   * Delegate window operations to WindowManagerDaemon  
   * Commands don't manage windows directly - they delegate to specialized daemons
   */
  private static async delegateToWindowManager(operation: string, params: any): Promise<WindowInfo> {
    // TODO: Implement actual daemon communication via WebSocket/message passing
    
    const daemonMessage = {
      type: 'daemon_request',
      target: 'window-manager', 
      operation,
      params,
      requestId: `connect_window_${Date.now()}`
    };
    
    // In real implementation, this would:
    // 1. Send message via WebSocket to WindowManagerDaemon  
    // 2. WindowManagerDaemon coordinates with BrowserManagerDaemon
    // 3. Returns window/tab information for collaborative sessions
    
    // Mock response for now
    const mockWindowInfo: WindowInfo = {
      windowId: `window_${Date.now()}`,
      tabId: `tab_${Date.now()}`,
      url: `localhost:9000?session=${params.sessionId}`,
      title: `Collaborative Session - ${params.sessionId.split('-').pop()}`,
      devToolsAccess: params.devtools || false
    };
    
    return mockWindowInfo;
  }

  /**
   * Delegate filesystem operations to ContinuumFileSystemDaemon
   * Commands don't touch filesystem directly - they use kernel services
   */
  private static async delegateToContinuumFileSystem(operation: string, params: any): Promise<any> {
    // TODO: Implement actual daemon communication
    
    const daemonMessage = {
      type: 'daemon_request',
      target: 'continuum-filesystem',
      operation,
      params,
      requestId: `connect_fs_${Date.now()}`
    };
    
    // In real implementation:
    // 1. ContinuumFileSystemDaemon manages .continuum directory structure
    // 2. Creates session-appropriate artifact directories  
    // 3. Handles file operations with proper permissions and organization
    
    return { success: true, path: params.artifactLocation };
  }

  /**
   * Legacy method - replaced by daemon delegation
   * @deprecated Use delegateToContinuumFileSystem instead
   */
  private static async getContinuumDirectoryConfig(): Promise<{
    rootPath: string;
    structure: string[];
  }> {
    try {
      // In a full implementation, this would call the ContinuumDirectoryDaemon
      // For now, discover .continuum location programmatically
      const homeDir = process.env.HOME || process.env.USERPROFILE || process.cwd();
      const continuumRoot = path.join(homeDir, '.continuum');
      
      // Ensure .continuum directory exists
      await fs.mkdir(continuumRoot, { recursive: true });
      
      // Get directory structure from daemon (simulated)
      const structure = [
        'sessions/portal',
        'sessions/validation',
        'sessions/user', 
        'sessions/personas',
        'config',
        'cache',
        'logs',
        'temp'
      ];

      // Create base directory structure using filesystem commands
      for (const dir of structure) {
        const dirPath = path.join(continuumRoot, dir);
        await fs.mkdir(dirPath, { recursive: true });
      }

      return {
        rootPath: continuumRoot,
        structure
      };

    } catch (error) {
      throw new Error(`Failed to get continuum directory config: ${error}`);
    }
  }

  /**
   * Create session using daemon's directory organization
   */
  private static async createSession(params: any, directoryConfig: any): Promise<{
    sessionId: string;
    sessionPath: string;
    artifactPath: string;
    sessionType: string;
    owner: string;
  }> {
    const sessionType = params.sessionType || 'user';
    const owner = params.owner || 'development';
    const sessionId = `${sessionType}-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    
    // Use daemon's directory organization
    let sessionBasePath: string;
    switch (sessionType) {
      case 'portal':
        sessionBasePath = path.join(directoryConfig.rootPath, 'sessions', 'portal');
        break;
      case 'validation':
        sessionBasePath = path.join(directoryConfig.rootPath, 'sessions', 'validation');
        break;
      case 'persona':
        sessionBasePath = path.join(directoryConfig.rootPath, 'sessions', 'personas', owner);
        break;
      default:
        sessionBasePath = path.join(directoryConfig.rootPath, 'sessions', 'user', owner);
    }

    const sessionPath = path.join(sessionBasePath, sessionId);
    
    // Create session directory structure using filesystem commands
    await fs.mkdir(sessionPath, { recursive: true });
    
    // Create artifact subdirectories 
    const artifactDirs = ['screenshots', 'logs', 'recordings', 'files', 'devtools', 'metadata'];
    for (const artifactDir of artifactDirs) {
      await fs.mkdir(path.join(sessionPath, artifactDir), { recursive: true });
    }

    // Create session metadata file
    const metadata = {
      sessionId,
      sessionType,
      owner,
      created: new Date().toISOString(),
      artifactDirs,
      config: params
    };

    await fs.writeFile(
      path.join(sessionPath, 'session.json'),
      JSON.stringify(metadata, null, 2)
    );

    return {
      sessionId,
      sessionPath,
      artifactPath: sessionPath,
      sessionType,
      owner
    };
  }

  /**
   * Launch browser using session information
   */
  private static async launchBrowser(sessionData: any, params: any): Promise<{
    launched: boolean;
    pid?: number;
    devtools?: boolean;
  }> {
    try {
      if (params.background) {
        // Don't launch browser in background mode
        return { launched: false };
      }

      // Use the platform's default browser launching
      const platform = process.platform;
      const url = 'http://localhost:9000';
      
      let browserCommand: string;
      switch (platform) {
        case 'darwin':
          browserCommand = `open "${url}"`;
          break;
        case 'win32':
          browserCommand = `start "${url}"`;
          break;
        default:
          browserCommand = `xdg-open "${url}"`;
      }

      // If devtools requested, add appropriate flags
      if (params.devtools) {
        switch (platform) {
          case 'darwin':
            browserCommand = `open -a "Google Chrome" "${url}" --args --auto-open-devtools-for-tabs`;
            break;
          case 'win32':
            browserCommand = `start chrome "${url}" --auto-open-devtools-for-tabs`;
            break;
          default:
            browserCommand = `google-chrome "${url}" --auto-open-devtools-for-tabs`;
        }
      }

      // Execute browser launch command
      const { exec } = await import('child_process');
      const { promisify } = await import('util');
      const execAsync = promisify(exec);
      
      await execAsync(browserCommand);

      // Log browser launch to session artifacts
      const logEntry = {
        timestamp: new Date().toISOString(),
        action: 'browser_launched',
        command: browserCommand,
        sessionId: sessionData.sessionId,
        url
      };

      await fs.writeFile(
        path.join(sessionData.sessionPath, 'logs', 'browser.log'),
        JSON.stringify(logEntry, null, 2)
      );

      return {
        launched: true,
        devtools: params.devtools || false
      };

    } catch (error) {
      // Log failure to session artifacts
      const errorEntry = {
        timestamp: new Date().toISOString(),
        action: 'browser_launch_failed',
        error: error instanceof Error ? error.message : String(error),
        sessionId: sessionData.sessionId
      };

      await fs.writeFile(
        path.join(sessionData.sessionPath, 'logs', 'browser-error.log'),
        JSON.stringify(errorEntry, null, 2)
      );

      return { launched: false };
    }
  }
}