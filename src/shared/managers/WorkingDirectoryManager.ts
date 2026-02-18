/**
 * Working Directory Manager - Single Source of Truth for All Paths
 * 
 * Centralizes all directory creation and path resolution logic.
 * Eliminates the scattered path creation that's causing silent failures.
 */

import path from 'path';
import fs from 'fs/promises';
import { execAsync } from '../utils/ProcessUtils';

export interface DirectoryStructure {
  root: string;
  continuum: string;
  jtag: string;
  currentUser: string;
  sessions: string;
  system: string;
  logs: string;
  screenshots: string;
  data: string;
  signals: string;
  performance: string;
}

export interface WorkingDirectoryConfig {
  projectRoot: string;
  activeExample?: string;
  sessionId?: string;
}

export class WorkingDirectoryManager {
  private config: WorkingDirectoryConfig;
  private structure?: DirectoryStructure;

  constructor(config: WorkingDirectoryConfig) {
    this.config = config;
  }

  /**
   * Get canonical directory structure
   */
  async getDirectoryStructure(): Promise<DirectoryStructure> {
    if (!this.structure) {
      await this.initializeStructure();
    }
    return this.structure!;
  }

  /**
   * Initialize directory structure based on project context
   */
  private async initializeStructure(): Promise<void> {
    // Determine the actual working directory
    const rootPath = await this.determineRootPath();
    
    // Build canonical structure
    const continuum = path.join(rootPath, '.continuum');
    const jtag = path.join(continuum, 'jtag');
    
    this.structure = {
      root: rootPath,
      continuum,
      jtag,
      currentUser: path.join(jtag, 'currentUser'),
      sessions: path.join(jtag, 'sessions'),
      system: path.join(jtag, 'system'),
      logs: path.join(jtag, 'currentUser', 'logs'),
      screenshots: path.join(jtag, 'currentUser', 'screenshots'),
      data: path.join(jtag, 'currentUser', 'data'),
      signals: path.join(jtag, 'signals'),
      performance: path.join(jtag, 'performance')
    };
  }

  /**
   * Smart root path determination
   */
  private async determineRootPath(): Promise<string> {
    // Priority order for root path detection:
    // 1. Explicit activeExample configuration
    // 2. Working directory with JTAG_WORKING_DIR env var
    // 3. Current working directory
    // 4. Project root fallback
    
    if (this.config.activeExample) {
      const examplePath = path.join(this.config.projectRoot, 'examples', this.config.activeExample);
      if (await this.directoryExists(examplePath)) {
        console.log(`📁 WorkingDirManager: Using active example: ${this.config.activeExample}`);
        return examplePath;
      }
    }

    const workingDir = process.env.JTAG_WORKING_DIR;
    if (workingDir) {
      const envPath = path.resolve(workingDir);
      if (await this.directoryExists(envPath)) {
        console.log(`📁 WorkingDirManager: Using JTAG_WORKING_DIR: ${workingDir}`);
        return envPath;
      }
    }

    const cwd = process.cwd();
    if (cwd.includes('examples/')) {
      console.log(`📁 WorkingDirManager: Using current working directory: ${cwd}`);
      return cwd;
    }

    console.log(`📁 WorkingDirManager: Using project root: ${this.config.projectRoot}`);
    return this.config.projectRoot;
  }

  /**
   * Create all necessary directories
   */
  async ensureDirectoryStructure(): Promise<void> {
    const structure = await this.getDirectoryStructure();
    
    // Create directories in dependency order
    const directories = [
      structure.continuum,
      structure.jtag,
      structure.sessions,
      structure.system,
      structure.signals,
      structure.performance
    ];

    // Create session-specific directories
    if (this.config.sessionId) {
      const userSessionPath = path.join(structure.sessions, 'user', this.config.sessionId);
      const systemSessionPath = path.join(structure.sessions, 'system', this.config.sessionId);
      
      directories.push(
        path.join(structure.sessions, 'user'),
        path.join(structure.sessions, 'system'),
        userSessionPath,
        systemSessionPath,
        path.join(userSessionPath, 'logs'),
        path.join(userSessionPath, 'screenshots'),
        path.join(userSessionPath, 'data'),
        path.join(systemSessionPath, 'logs')
      );
    }

    // Create all directories
    for (const dir of directories) {
      await fs.mkdir(dir, { recursive: true });
    }

    // Create currentUser symlink to active session
    if (this.config.sessionId) {
      await this.createCurrentUserSymlink();
    }

    console.log(`✅ WorkingDirManager: Ensured directory structure at ${structure.root}`);
  }

  /**
   * Create currentUser symlink to active session
   */
  private async createCurrentUserSymlink(): Promise<void> {
    const structure = await this.getDirectoryStructure();
    const currentUserLink = structure.currentUser;
    const sessionTarget = path.join('sessions', 'user', this.config.sessionId!);
    
    try {
      // Remove existing symlink
      await fs.unlink(currentUserLink).catch(() => {});
      
      // Create new symlink
      await fs.symlink(sessionTarget, currentUserLink);
      
      console.log(`🔗 WorkingDirManager: Created currentUser symlink: ${currentUserLink} → ${sessionTarget}`);
    } catch (error: any) {
      console.warn(`⚠️ WorkingDirManager: Failed to create currentUser symlink: ${error.message}`);
    }
  }

  /**
   * Get specific directory paths
   */
  async getLogsPath(): Promise<string> {
    const structure = await this.getDirectoryStructure();
    return structure.logs;
  }

  async getScreenshotsPath(): Promise<string> {
    const structure = await this.getDirectoryStructure();
    return structure.screenshots;
  }

  async getDataPath(): Promise<string> {
    const structure = await this.getDirectoryStructure();
    return structure.data;
  }

  async getSignalsPath(): Promise<string> {
    const structure = await this.getDirectoryStructure();
    return structure.signals;
  }

  async getPerformancePath(): Promise<string> {
    const structure = await this.getDirectoryStructure();
    return structure.performance;
  }

  /**
   * Detect existing sessions and their locations
   */
  async detectActiveSessions(): Promise<{ path: string; sessionId: string }[]> {
    const structure = await this.getDirectoryStructure();
    const sessions: { path: string; sessionId: string }[] = [];
    
    try {
      const userSessionsPath = path.join(structure.sessions, 'user');
      const sessionDirs = await fs.readdir(userSessionsPath);
      
      for (const sessionId of sessionDirs) {
        const sessionPath = path.join(userSessionsPath, sessionId);
        const stat = await fs.stat(sessionPath);
        if (stat.isDirectory()) {
          sessions.push({ path: sessionPath, sessionId });
        }
      }
    } catch (error) {
      // Sessions directory doesn't exist yet
    }
    
    return sessions;
  }

  /**
   * Find the actual current session (follow symlinks)
   */
  async getCurrentSession(): Promise<{ path: string; sessionId: string } | null> {
    const structure = await this.getDirectoryStructure();
    
    try {
      const realPath = await fs.realpath(structure.currentUser);
      const sessionId = path.basename(realPath);
      return { path: realPath, sessionId };
    } catch (error) {
      return null;
    }
  }

  /**
   * Utility: Check if directory exists
   */
  private async directoryExists(dirPath: string): Promise<boolean> {
    try {
      const stat = await fs.stat(dirPath);
      return stat.isDirectory();
    } catch {
      return false;
    }
  }

  /**
   * Factory method for common configurations
   */
  static createForProject(projectRoot: string, activeExample?: string): WorkingDirectoryManager {
    return new WorkingDirectoryManager({ projectRoot, activeExample });
  }

  static createForSession(projectRoot: string, sessionId: string, activeExample?: string): WorkingDirectoryManager {
    return new WorkingDirectoryManager({ projectRoot, sessionId, activeExample });
  }

  /**
   * Auto-detect configuration from environment
   */
  static async createFromEnvironment(): Promise<WorkingDirectoryManager> {
    const cwd = process.cwd();
    let projectRoot = cwd;
    let activeExample: string | undefined;

    // Detect if we're in an example directory
    if (cwd.includes('/examples/')) {
      const exampleMatch = cwd.match(/\/examples\/([^\/]+)/);
      if (exampleMatch) {
        activeExample = exampleMatch[1];
        projectRoot = cwd.split('/examples/')[0];
      }
    }

    // Check for JTAG_WORKING_DIR
    const workingDir = process.env.JTAG_WORKING_DIR;
    if (workingDir && workingDir.includes('/examples/')) {
      const exampleMatch = workingDir.match(/\/examples\/([^\/]+)/);
      if (exampleMatch) {
        activeExample = exampleMatch[1];
        projectRoot = path.dirname(path.dirname(workingDir));
      }
    }

    return new WorkingDirectoryManager({ projectRoot, activeExample });
  }
}