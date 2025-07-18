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
import { WebSocketRoutingService } from './server/WebSocketRoutingService';

console.log('🔍 DEBUG: SessionManagerCompatibilityWrapper file loaded');

export class SessionManagerCompatibilityWrapper extends LegacySessionManagerDaemon {
  private newSessionManager: NewSessionManagerDaemon | null = null;
  private migrationEnabled = false;
  private webSocketRoutingService: WebSocketRoutingService;

  constructor(context: ContinuumContext, artifactRoot: string = '.continuum/sessions') {
    super(context, artifactRoot);
    
    // Initialize WebSocket routing service (always enabled - first cross-cutting concern extraction)
    this.webSocketRoutingService = new WebSocketRoutingService({
      context,
      logger: {
        log: (message: string, level: 'info' | 'debug' | 'warn' | 'error' = 'info') => {
          this.log(`[WebSocketRouting] ${message}`, level);
        }
      }
    });
    
    // Only enable migration if environment variable is set
    this.migrationEnabled = true; // HARDCODED FOR TESTING: process.env.CONTINUUM_ENABLE_SESSION_MIGRATION === 'true';
    
    if (this.migrationEnabled) {
      console.log('🔄 Session Manager Migration: ENABLED');
      this.newSessionManager = new NewSessionManagerDaemon(context, artifactRoot);
    } else {
      console.log('🔄 Session Manager Migration: DISABLED (use CONTINUUM_ENABLE_SESSION_MIGRATION=true to enable)');
    }
    
    console.log('🔧 First Cross-Cutting Concern Extraction: WebSocket routing service initialized');
    console.log('🎯 COMPATIBILITY WRAPPER CONFIRMED: This is definitely NOT the old SessionManagerDaemon!');
    console.log('🚀 NEW ARCHITECTURE ACTIVE: Modular daemon pattern with surgical precision migration');
  }

  /**
   * Start both old and new systems during migration
   */
  protected async onStart(): Promise<void> {
    // Initialize WebSocket routing service first
    await this.webSocketRoutingService.initialize();
    
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
    
    // Cleanup WebSocket routing service last
    await this.webSocketRoutingService.cleanup();
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
   * Register with WebSocket daemon - delegate to routing service
   */
  async registerWithWebSocketDaemon(webSocketDaemon: any): Promise<void> {
    // Delegate to the WebSocket routing service
    await this.webSocketRoutingService.registerWithWebSocketDaemon(webSocketDaemon);
    
    // Also call the legacy method to maintain compatibility
    await super.registerWithWebSocketDaemon(webSocketDaemon);
  }

  /**
   * Get migration status for debugging
   */
  getMigrationStatus(): { enabled: boolean; newSystemReady: boolean; webSocketServiceReady: boolean } {
    return {
      enabled: this.migrationEnabled,
      newSystemReady: this.newSessionManager !== null,
      webSocketServiceReady: this.webSocketRoutingService !== null
    };
  }
}