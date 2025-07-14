// ISSUES: 1 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// 🎯 ARCHITECTURAL FEATURE: Universal shared session context for all command integrations
/**
 * Shared Session Context Provider
 * 
 * Provides universal session context that all command integrations can use by default.
 * Eliminates the need for explicit session management in CLI, REST, WebSocket, MCP, etc.
 */

import { ContinuumContext, continuumContextFactory } from '../types/shared/core/ContinuumTypes';
import { randomUUID } from 'crypto';
import * as fs from 'fs';
import * as path from 'path';

interface SharedSessionInfo {
  sessionId: string;
  userId?: string;
  interface: string;
  logs: {
    browser: string;
    server: string;
  };
  screenshots: string;
  createdAt: string;
  lastActivity: string;
}

class SharedSessionContextProvider {
  private static instance: SharedSessionContextProvider;
  private cachedContext: ContinuumContext | null = null;
  private lastRefresh: number = 0;
  private readonly CACHE_TTL = 30000; // 30 seconds

  static getInstance(): SharedSessionContextProvider {
    if (!SharedSessionContextProvider.instance) {
      SharedSessionContextProvider.instance = new SharedSessionContextProvider();
    }
    return SharedSessionContextProvider.instance;
  }

  /**
   * Get the current shared session context
   * Automatically discovers and caches the active session
   */
  async getSharedContext(): Promise<ContinuumContext> {
    const now = Date.now();
    
    // Return cached context if still valid
    if (this.cachedContext && (now - this.lastRefresh) < this.CACHE_TTL) {
      return this.cachedContext;
    }

    try {
      // Discover active session from filesystem
      const sessionInfo = await this.discoverActiveSession();
      console.log('🔍 SharedSessionContext: Discovered session info:', sessionInfo);
      
      if (sessionInfo) {
        this.cachedContext = {
          sessionId: randomUUID(),
          userId: sessionInfo.userId || 'development-user',
          interface: sessionInfo.interface,
          screenshotsPath: sessionInfo.screenshots,
          logsPath: sessionInfo.logs,
          sharedSession: true,
          discoveredAt: new Date().toISOString()
        };
      } else {
        // Fallback context when no session is found
        this.cachedContext = {
          sessionId: randomUUID(),
          userId: 'development-user',
          interface: 'http://localhost:9000',
          sharedSession: false,
          fallback: true,
          discoveredAt: new Date().toISOString()
        };
      }

      this.lastRefresh = now;
      return this.cachedContext;

    } catch (error) {
      console.warn('Failed to discover shared session context:', error);
      
      // Return minimal fallback context
      return continuumContextFactory.create({
        sessionId: randomUUID(),
        userId: 'development-user',
        sharedSession: false,
        error: true,
        discoveredAt: new Date().toISOString()
      });
    }
  }

  /**
   * Merge provided context with shared context
   * Provided context takes precedence over shared context
   */
  async mergeWithSharedContext(providedContext?: ContinuumContext): Promise<ContinuumContext> {
    // Use factory to create default context if not provided
    const defaultContext = continuumContextFactory.create({
      ...(providedContext?.sessionId && { sessionId: providedContext.sessionId }),
      environment: 'server'
    });
    
    const contextToUse = providedContext ? 
      continuumContextFactory.merge(defaultContext, providedContext) : 
      defaultContext;
    
    const sharedContext = await this.getSharedContext();
    
    return continuumContextFactory.merge(sharedContext, {
      ...contextToUse,
      mergedFromShared: true
    });
  }

  /**
   * Invalidate cached context to force refresh
   */
  invalidateCache(): void {
    this.cachedContext = null;
    this.lastRefresh = 0;
  }

  /**
   * Discover active session from filesystem
   */
  private async discoverActiveSession(): Promise<SharedSessionInfo | null> {
    try {
      // Look for active session in .continuum/sessions/user/shared/
      const sessionsPath = path.join(process.cwd(), '.continuum', 'sessions', 'user', 'shared');
      console.log(`🔍 SharedSessionContext: Looking for sessions in: ${sessionsPath}`);
      
      if (!fs.existsSync(sessionsPath)) {
        console.log(`🔍 SharedSessionContext: Sessions path does not exist`);
        return null;
      }

      const sessionDirs = fs.readdirSync(sessionsPath, { withFileTypes: true })
        .filter(dirent => dirent.isDirectory())
        .map(dirent => dirent.name)
        .filter(name => name.startsWith('development-shared-'));

      console.log(`🔍 SharedSessionContext: Found ${sessionDirs.length} session directories:`, sessionDirs);

      if (sessionDirs.length === 0) {
        console.log(`🔍 SharedSessionContext: No development-shared sessions found`);
        return null;
      }

      // Use the most recently modified session
      const sessionDir = sessionDirs
        .map(dir => ({
          name: dir,
          path: path.join(sessionsPath, dir),
          mtime: fs.statSync(path.join(sessionsPath, dir)).mtime
        }))
        .sort((a, b) => b.mtime.getTime() - a.mtime.getTime())[0];

      const sessionPath = sessionDir.path;
      const sessionId = sessionDir.name;

      // Check if session has required structure
      const logsPath = path.join(sessionPath, 'logs');
      const screenshotsPath = path.join(sessionPath, 'screenshots');

      if (!fs.existsSync(logsPath)) {
        return null;
      }

      return {
        sessionId,
        interface: 'http://localhost:9000',
        logs: {
          browser: path.join(logsPath, 'browser.log'),
          server: path.join(logsPath, 'server.log')
        },
        screenshots: screenshotsPath,
        createdAt: sessionDir.mtime.toISOString(),
        lastActivity: new Date().toISOString()
      };

    } catch (error) {
      console.warn('Error discovering active session:', error);
      return null;
    }
  }
}

// Export singleton instance and convenience functions
export const sharedSessionContext = SharedSessionContextProvider.getInstance();

/**
 * Get shared session context for command execution
 */
export async function getSharedSessionContext(): Promise<ContinuumContext> {
  return sharedSessionContext.getSharedContext();
}

/**
 * Merge provided context with shared session context
 */
export async function mergeWithSharedContext(context?: ContinuumContext): Promise<ContinuumContext> {
  return sharedSessionContext.mergeWithSharedContext(context);
}

/**
 * Invalidate shared session context cache
 */
export function invalidateSharedSessionCache(): void {
  sharedSessionContext.invalidateCache();
}