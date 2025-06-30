/**
 * Browser Manager Session Integration
 * 
 * Integrates browser management with existing session and artifact systems
 * Does not duplicate functionality - uses existing session infrastructure
 */

import { IntelligentBrowserManager, LaunchContext } from './IntelligentBrowserManager.js';

export interface ExistingSessionInfo {
  sessionId: string;
  artifactPath: string;
  type: 'portal' | 'validation' | 'user' | 'persona';
  owner: string;
  context?: string;
}

export class BrowserSessionIntegration {
  private browserManager: IntelligentBrowserManager;

  constructor() {
    this.browserManager = new IntelligentBrowserManager();
  }

  /**
   * Launch browser using existing session info from connection
   */
  async launchWithSessionInfo(
    sessionInfo: ExistingSessionInfo,
    options: {
      devTools?: boolean;
      devToolsTabs?: string[];
      url?: string;
    } = {}
  ): Promise<{ success: boolean; error?: string }> {
    try {
      // Build launch context from session info
      const context: LaunchContext = {
        source: this.mapSessionTypeToSource(sessionInfo.type),
        purpose: this.determinePurpose(sessionInfo.type, options.devTools),
        url: options.url || 'http://localhost:9000',
        devTools: options.devTools ? {
          enabled: true,
          openConsole: options.devToolsTabs?.includes('console') ?? true,
          openNetwork: options.devToolsTabs?.includes('network') ?? false,
          openSources: options.devToolsTabs?.includes('sources') ?? false
        } : { enabled: false }
      };

      console.log(`🚀 Launching browser for session: ${sessionInfo.sessionId}`);
      console.log(`📁 Artifacts will be saved to: ${sessionInfo.artifactPath}`);

      const result = await this.browserManager.launchIntelligent(context);

      if (result.success) {
        // Register browser with session for artifact coordination
        this.registerBrowserWithSession(sessionInfo, result);
      }

      return {
        success: result.success,
        error: result.success ? undefined : result.error
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Browser launch failed: ${errorMessage}`
      };
    }
  }

  /**
   * Register browser connection with session info
   */
  registerBrowserWithSession(sessionInfo: ExistingSessionInfo, launchResult: any): void {
    // Use existing session system's connection identity
    const connectionId = `browser-${sessionInfo.sessionId}`;
    
    this.browserManager.registerConnection(connectionId);
    
    console.log(`🔗 Browser registered with session ${sessionInfo.sessionId}`);
    console.log(`📋 Type: ${sessionInfo.type}, Owner: ${sessionInfo.owner}`);
    
    if (sessionInfo.context) {
      console.log(`📋 Context: ${sessionInfo.context}`);
    }
  }

  /**
   * Get browser status for session diagnostics
   */
  getBrowserStatusForSession(sessionId: string): {
    connected: boolean;
    healthy: boolean;
    lastSeen: Date | null;
  } {
    const status = this.browserManager.getConnectionStatus();
    
    return {
      connected: status.isConnected,
      healthy: status.isHealthy,
      lastSeen: status.lastSeen
    };
  }

  /**
   * Map session type to launch source
   */
  private mapSessionTypeToSource(type: ExistingSessionInfo['type']): LaunchContext['source'] {
    switch (type) {
      case 'portal':
        return 'portal';
      case 'validation':
        return 'git-hook';
      case 'user':
        return 'manual';
      case 'persona':
        return 'development';
      default:
        return 'manual';
    }
  }

  /**
   * Determine launch purpose based on session type and options
   */
  private determinePurpose(type: ExistingSessionInfo['type'], devTools?: boolean): LaunchContext['purpose'] {
    if (devTools) {
      return 'debugging';
    }

    switch (type) {
      case 'portal':
        return 'debugging';
      case 'validation':
        return 'testing';
      case 'user':
        return 'development';
      case 'persona':
        return 'development';
      default:
        return 'development';
    }
  }

  /**
   * Ensure browser is ready for session work
   */
  async ensureBrowserReadyForSession(
    sessionInfo: ExistingSessionInfo,
    timeoutMs: number = 15000
  ): Promise<{ ready: boolean; error?: string }> {
    try {
      // Check if browser is already ready
      const status = this.getBrowserStatusForSession(sessionInfo.sessionId);
      if (status.connected && status.healthy) {
        return { ready: true };
      }

      // Launch browser with session info
      const launchResult = await this.launchWithSessionInfo(sessionInfo);
      if (!launchResult.success) {
        return {
          ready: false,
          error: launchResult.error
        };
      }

      // Wait for connection
      const connected = await this.browserManager.waitForConnection(timeoutMs);
      
      return {
        ready: connected,
        error: connected ? undefined : `Browser connection timeout after ${timeoutMs}ms`
      };

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        ready: false,
        error: `Browser readiness check failed: ${errorMessage}`
      };
    }
  }

  /**
   * Clean shutdown - let existing session system handle cleanup
   */
  async shutdown(): Promise<void> {
    // Don't cleanup artifacts - that's handled by the existing session system
    // Just disconnect browser cleanly
    this.browserManager.registerDisconnection();
    console.log('🔌 Browser manager disconnected from session');
  }
}