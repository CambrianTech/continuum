/**
 * CleanupService - Handles session cleanup and monitoring for SessionManager
 * 
 * This service extracts session cleanup responsibilities from SessionManagerDaemon
 * using in-place method delegation for gradual refactoring.
 */

import * as fs from 'fs/promises';
import { ContinuumContext } from '../../../types/shared/core/ContinuumTypes';
import { BrowserSession } from '../SessionManagerDaemon';

export class CleanupService {
  private cleanupInterval?: NodeJS.Timeout;
  private sessions: Map<string, BrowserSession>;
  private logFunction: (message: string, level?: 'error' | 'debug' | 'info' | 'warn') => void;
  
  constructor(
    _context: ContinuumContext,
    sessions: Map<string, BrowserSession>,
    logFunction: (message: string, level?: 'error' | 'debug' | 'info' | 'warn') => void
  ) {
    this.sessions = sessions;
    this.logFunction = logFunction;
  }
  
  /**
   * Starts cleanup monitoring for sessions
   */
  startCleanupMonitoring(): void {
    this.cleanupInterval = setInterval(async () => {
      const currentTime = Date.now();
      
      for (const [sessionId, session] of this.sessions) {
        if (session.shouldAutoCleanup) {
          const timeSinceLastActive = currentTime - session.lastActive.getTime();
          
          if (timeSinceLastActive > session.cleanupAfterMs) {
            this.logFunction(`🧹 Auto-cleaning session ${sessionId} (inactive for ${timeSinceLastActive}ms)`, 'info');
            
            try {
              await this.cleanupSessionArtifacts(session);
              this.sessions.delete(sessionId);
            } catch (error) {
              this.logFunction(`❌ Failed to cleanup session ${sessionId}: ${error}`, 'error');
            }
          }
        }
      }
    }, 60000); // Check every minute
    
    this.logFunction('📅 Started session cleanup monitoring', 'info');
  }
  
  /**
   * Stops cleanup monitoring
   */
  stopCleanupMonitoring(): void {
    if (this.cleanupInterval) {
      clearInterval(this.cleanupInterval);
      this.cleanupInterval = undefined as any;
      this.logFunction('🛑 Stopped session cleanup monitoring', 'info');
    }
  }
  
  /**
   * Cleans up artifacts for a specific session
   */
  async cleanupSessionArtifacts(session: BrowserSession): Promise<void> {
    try {
      // Remove session directory and all artifacts
      await fs.rm(session.artifacts.storageDir, { recursive: true, force: true });
      
      this.logFunction(`🧹 Cleaned up artifacts for session ${session.id}`, 'info');
    } catch (error) {
      this.logFunction(`❌ Failed to cleanup artifacts for session ${session.id}: ${error}`, 'error');
      throw error;
    }
  }
  
  /**
   * Manually cleanup a session by ID
   */
  async cleanupSession(sessionId: string): Promise<void> {
    const session = this.sessions.get(sessionId);
    if (!session) {
      throw new Error(`Session ${sessionId} not found`);
    }
    
    await this.cleanupSessionArtifacts(session);
    this.sessions.delete(sessionId);
    
    this.logFunction(`🧹 Manually cleaned up session ${sessionId}`, 'info');
  }
  
  /**
   * Gets cleanup status for all sessions
   */
  getCleanupStatus(): {
    isMonitoring: boolean;
    sessionsWithCleanup: number;
    totalSessions: number;
    nextCleanupEstimate?: Date;
  } {
    const sessionsWithCleanup = Array.from(this.sessions.values()).filter(s => s.shouldAutoCleanup);
    
    let nextCleanupEstimate: Date | undefined = undefined;
    if (sessionsWithCleanup.length > 0) {
      const currentTime = Date.now();
      const nextCleanup = Math.min(...sessionsWithCleanup.map(s => 
        s.lastActive.getTime() + s.cleanupAfterMs
      ));
      
      if (nextCleanup > currentTime) {
        nextCleanupEstimate = new Date(nextCleanup);
      }
    }
    
    const result: {
      isMonitoring: boolean;
      sessionsWithCleanup: number;
      totalSessions: number;
      nextCleanupEstimate?: Date;
    } = {
      isMonitoring: this.cleanupInterval !== undefined,
      sessionsWithCleanup: sessionsWithCleanup.length,
      totalSessions: this.sessions.size
    };
    
    if (nextCleanupEstimate) {
      result.nextCleanupEstimate = nextCleanupEstimate;
    }
    
    return result;
  }
  
  /**
   * Updates session last active time
   */
  updateSessionActivity(sessionId: string): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.lastActive = new Date();
      this.logFunction(`📅 Updated activity for session ${sessionId}`, 'debug');
    }
  }
  
  /**
   * Configures cleanup settings for a session
   */
  configureSessionCleanup(sessionId: string, options: {
    shouldAutoCleanup: boolean;
    cleanupAfterMs: number;
  }): void {
    const session = this.sessions.get(sessionId);
    if (session) {
      session.shouldAutoCleanup = options.shouldAutoCleanup;
      session.cleanupAfterMs = options.cleanupAfterMs;
      
      this.logFunction(`⚙️ Configured cleanup for session ${sessionId}: auto=${options.shouldAutoCleanup}, timeout=${options.cleanupAfterMs}ms`, 'info');
    }
  }
}