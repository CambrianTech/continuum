/**
 * DirectoryService - Handles directory creation and file operations for SessionManager
 * 
 * This service extracts directory management responsibilities from SessionManagerDaemon
 * using in-place method delegation for gradual refactoring.
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';

export class DirectoryService {
  private artifactRoot: string;
  
  constructor(_context: ContinuumContext, artifactRoot: string = '.continuum/sessions') {
    this.artifactRoot = artifactRoot;
  }
  
  /**
   * Creates complete directory structure for a session
   */
  async createSessionStorage(
    sessionId: string,
    sessionType: string,
    owner: string,
    context?: string
  ): Promise<{
    storageDir: string;
    logs: { server: string[]; client: string[] };
    screenshots: string[];
    files: string[];
    recordings: string[];
    devtools: string[];
  }> {
    // Build organized path based on session type
    let sessionPath: string;
    
    switch (sessionType) {
      case 'development':
      case 'test':
        sessionPath = path.join(this.artifactRoot, 'user', owner, sessionId);
        break;
      case 'persona':
        sessionPath = path.join(this.artifactRoot, 'personas', owner, sessionId);
        break;
      case 'portal':
        sessionPath = path.join(this.artifactRoot, 'portals', owner, context || 'default', sessionId);
        break;
      case 'git-hook':
        sessionPath = path.join(this.artifactRoot, 'git-hooks', owner, context || 'default', sessionId);
        break;
      default:
        sessionPath = path.join(this.artifactRoot, 'unknown', owner, sessionId);
    }
    
    // Create the session directory
    await fs.mkdir(sessionPath, { recursive: true });
    
    // Create subdirectories
    const subdirs = ['logs', 'screenshots', 'files', 'recordings', 'devtools'];
    for (const subdir of subdirs) {
      await fs.mkdir(path.join(sessionPath, subdir), { recursive: true });
    }
    
    // Create initial log files
    const sessionStartMessage = `# Continuum Session Log
# Session: ${sessionId}
# Created: ${new Date().toISOString()}
# Type: ${sessionType}
# Owner: ${owner}
#
# Session started at ${new Date().toISOString()}

`;
    
    const serverLogPath = path.join(sessionPath, 'logs', 'server.log');
    await fs.writeFile(serverLogPath, sessionStartMessage);
    
    return {
      storageDir: sessionPath,
      logs: { 
        server: [serverLogPath], 
        client: [] 
      },
      screenshots: [],
      files: [],
      recordings: [],
      devtools: []
    };
  }
  
  /**
   * Initializes the base directory structure
   */
  async initializeDirectoryStructure(): Promise<void> {
    const baseDirectories = [
      'user',
      'personas', 
      'portals',
      'git-hooks',
      'unknown'
    ];
    
    for (const dir of baseDirectories) {
      await fs.mkdir(path.join(this.artifactRoot, dir), { recursive: true });
    }
  }
  
  /**
   * Generates an artifact filename with timestamp
   */
  generateArtifactFilename(type: string, source: string, timestamp: Date): string {
    const dateStr = timestamp.toISOString().split('T')[0];
    const timeStr = timestamp.toISOString().split('T')[1].split('.')[0].replace(/:/g, '-');
    return `${type}_${source}_${dateStr}_${timeStr}`;
  }
  
  /**
   * Adds an artifact to a session's storage
   */
  async addArtifact(
    sessionId: string,
    artifactType: 'screenshot' | 'file' | 'recording' | 'devtools',
    data: Buffer | string,
    metadata: { source: string; timestamp: Date; filename?: string }
  ): Promise<string> {
    const session = await this.getSessionStorageInfo(sessionId);
    if (!session) {
      throw new Error(`Session ${sessionId} not found`);
    }
    
    const filename = metadata.filename || 
      this.generateArtifactFilename(artifactType, metadata.source, metadata.timestamp);
    
    const artifactPath = path.join(session.storageDir, `${artifactType}s`, filename);
    
    if (typeof data === 'string') {
      await fs.writeFile(artifactPath, data, 'utf8');
    } else {
      await fs.writeFile(artifactPath, data);
    }
    
    return artifactPath;
  }
  
  /**
   * Gets session storage information
   */
  private async getSessionStorageInfo(sessionId: string): Promise<{ storageDir: string } | null> {
    // This would typically query the session registry
    // For now, we'll implement a basic path search
    const searchPaths = [
      path.join(this.artifactRoot, 'user', 'shared', sessionId),
      path.join(this.artifactRoot, 'user', 'development', sessionId),
      path.join(this.artifactRoot, 'personas', '*', sessionId),
      path.join(this.artifactRoot, 'portals', '*', '*', sessionId),
      path.join(this.artifactRoot, 'git-hooks', '*', '*', sessionId)
    ];
    
    for (const searchPath of searchPaths) {
      try {
        const stat = await fs.stat(searchPath);
        if (stat.isDirectory()) {
          return { storageDir: searchPath };
        }
      } catch (error) {
        // Continue searching
      }
    }
    
    return null;
  }
}