/**
 * Connect Command - Proper Implementation
 * 
 * Low-level command that uses ContinuumDirectoryDaemon for session management
 * Creates sessions, manages .continuum filesystem, launches browsers
 */

import { BaseCommand, CommandDefinition, CommandContext, CommandResult } from '../../core/base-command/BaseCommand';
import { FileWriteCommand } from '../../file/write/FileWriteCommand';
import { FileReadCommand } from '../../file/read/FileReadCommand';
import * as path from 'path';
import * as fs from 'fs/promises';

export class ConnectCommand extends BaseCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'connect',
      category: 'development',
      icon: '🔌',
      description: 'Connect to Continuum system with session management and browser launching',
      parameters: { 
        sessionType: 'string',
        owner: 'string', 
        devtools: 'boolean',
        background: 'boolean'
      },
      examples: [
        { description: 'Basic connection', command: 'connect' },
        { description: 'Connect with DevTools', command: 'connect --devtools' },
        { description: 'Portal session', command: 'connect --sessionType=portal --owner=claude' }
      ],
      usage: 'Create session, organize .continuum directory, and launch browser'
    };
  }

  static async execute(params: any = {}, context?: CommandContext): Promise<CommandResult> {
    try {
      // 1. Get .continuum directory configuration from ContinuumDirectoryDaemon
      const directoryConfig = await this.getContinuumDirectoryConfig();
      
      // 2. Create session using daemon's directory organization
      const sessionData = await this.createSession(params, directoryConfig);
      
      // 3. Launch browser using session information
      const browserResult = await this.launchBrowser(sessionData, params);
      
      // 4. Return complete connection information
      return this.createSuccessResult(
        `Connected successfully - Session ${sessionData.sessionId}`,
        {
          sessionId: sessionData.sessionId,
          sessionPath: sessionData.sessionPath,
          artifactPath: sessionData.artifactPath,
          browserUrl: 'http://localhost:9000',
          devtools: params.devtools || false,
          browser: browserResult,
          continuumDir: directoryConfig.rootPath
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`Connection failed: ${errorMessage}`);
    }
  }

  /**
   * Get .continuum directory configuration from daemon
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