/**
 * Session Manager Compatibility Wrapper
 * 
 * Surgical precision: This wrapper maintains exact compatibility with the existing system
 * while gradually delegating to the new elegant architecture.
 * 
 * PRINCIPLE: First, do no harm. Every existing method call must continue to work exactly as before.
 */

import { SessionManagerDaemon as LegacySessionManagerDaemon } from './SessionManagerDaemon';
import { SessionManagerDaemon as NewSessionManagerDaemon } from './server/SessionManagerDaemon';
import { ContinuumContext } from '../../types/shared/core/ContinuumTypes';
import { DaemonMessage, DaemonResponse } from '../base/DaemonProtocol';

export class SessionManagerCompatibilityWrapper extends LegacySessionManagerDaemon {
  private newSessionManager: NewSessionManagerDaemon | null = null;
  private migrationEnabled = false;

  constructor(context: ContinuumContext, artifactRoot: string = '.continuum/sessions') {
    super(context, artifactRoot);
    
    // Only enable migration if environment variable is set
    this.migrationEnabled = process.env.CONTINUUM_ENABLE_SESSION_MIGRATION === 'true';
    
    if (this.migrationEnabled) {
      console.log('🔄 Session Manager Migration: ENABLED');
      this.newSessionManager = new NewSessionManagerDaemon(context, artifactRoot);
    } else {
      console.log('🔄 Session Manager Migration: DISABLED (use CONTINUUM_ENABLE_SESSION_MIGRATION=true to enable)');
    }
  }

  /**
   * Start both old and new systems during migration
   */
  protected async onStart(): Promise<void> {
    // Always start the legacy system first
    await super.onStart();
    
    // Start new system if migration is enabled
    if (this.migrationEnabled && this.newSessionManager) {
      try {
        await this.newSessionManager.start();
        console.log('✅ New session manager started successfully');
      } catch (error) {
        console.error('❌ New session manager failed to start:', error);
        // Continue with legacy system only
        this.migrationEnabled = false;
        this.newSessionManager = null;
      }
    }
  }

  /**
   * Stop both systems during migration
   */
  protected async onStop(): Promise<void> {
    // Stop new system first
    if (this.newSessionManager) {
      try {
        await this.newSessionManager.stop();
        console.log('✅ New session manager stopped');
      } catch (error) {
        console.error('❌ Error stopping new session manager:', error);
      }
    }
    
    // Stop legacy system
    await super.onStop();
  }

  /**
   * Message handling with gradual migration
   */
  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse<any>> {
    // For now, always use legacy system
    // TODO: Gradually migrate specific message types to new system
    
    if (this.migrationEnabled && this.newSessionManager) {
      // Log message types to understand usage patterns
      console.log(`📊 Message type: ${message.type} (legacy handling)`);
    }
    
    return await super.handleMessage(message);
  }

  /**
   * Maintain exact compatibility with existing API
   */
  getSession(sessionId: string): any {
    return super.getSession(sessionId);
  }

  getLatestSession(criteria: any): any {
    return super.getLatestSession(criteria);
  }

  async stopSession(sessionId: string, options: any): Promise<any> {
    return await super.stopSession(sessionId, options);
  }

  onSessionEvent(listener: (event: any) => void): void {
    return super.onSessionEvent(listener);
  }

  offSessionEvent(listener: (event: any) => void): void {
    return super.offSessionEvent(listener);
  }

  /**
   * Get migration status for debugging
   */
  getMigrationStatus(): { enabled: boolean; newSystemReady: boolean } {
    return {
      enabled: this.migrationEnabled,
      newSystemReady: this.newSessionManager !== null
    };
  }
}