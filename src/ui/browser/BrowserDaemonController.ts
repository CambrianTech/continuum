/**
 * BrowserDaemonController - Orchestrates browser daemon system
 * 
 * Controls the dual implementation pattern for safe migration:
 * - Feature flag controlled daemon activation
 * - Fallback to legacy monolithic implementation
 * - Centralized daemon management and routing
 * 
 * Migration Strategy:
 * 1. Initialize with all daemons disabled (legacy behavior)
 * 2. Enable daemons one by one based on feature flags
 * 3. Route messages to appropriate implementation
 * 4. Provide rollback mechanisms
 */

import { BrowserDaemonManager } from './base/BrowserDaemonManager';
import { BrowserConsoleDaemon } from './console/BrowserConsoleDaemon';
import { BrowserWebSocketDaemon } from './websocket/BrowserWebSocketDaemon';
import { BrowserFeatureFlags } from './BrowserFeatureFlags';

interface DaemonControllerConfig {
  enableConsoleDaemon?: boolean;
  enableWebSocketDaemon?: boolean;
  enableCommandDaemon?: boolean;
  enableWidgetDaemon?: boolean;
  enableSessionDaemon?: boolean;
  enableHealthDaemon?: boolean;
}

export class BrowserDaemonController {
  private daemonManager: BrowserDaemonManager;
  private consoleDaemon: BrowserConsoleDaemon | null = null;
  private websocketDaemon: BrowserWebSocketDaemon | null = null;
  private isInitialized = false;
  private config: DaemonControllerConfig = {};

  constructor() {
    this.daemonManager = new BrowserDaemonManager();
  }

  /**
   * Initialize the daemon system based on feature flags
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      console.warn('BrowserDaemonController already initialized');
      return;
    }

    console.log('🏗️ BrowserDaemonController: Initializing with feature flags...');

    // Read feature flags to determine which daemons to enable
    this.config = {
      enableConsoleDaemon: BrowserFeatureFlags.CONSOLE_DAEMON_ENABLED,
      enableWebSocketDaemon: BrowserFeatureFlags.WEBSOCKET_DAEMON_ENABLED,
      enableCommandDaemon: BrowserFeatureFlags.COMMAND_DAEMON_ENABLED,
      enableWidgetDaemon: BrowserFeatureFlags.WIDGET_DAEMON_ENABLED,
      enableSessionDaemon: BrowserFeatureFlags.SESSION_DAEMON_ENABLED,
      enableHealthDaemon: BrowserFeatureFlags.HEALTH_DAEMON_ENABLED
    };

    if (BrowserFeatureFlags.isDebugMode) {
      console.log('🏁 Browser Daemon Configuration:', this.config);
    }

    // Initialize console daemon if enabled
    if (this.config.enableConsoleDaemon) {
      await this.initializeConsoleDaemon();
    }

    // Start daemon manager
    // await this.daemonManager.start(); // TODO: Implement start method in BrowserDaemonManager

    this.isInitialized = true;
    console.log(`✅ BrowserDaemonController: Initialized with ${this.getActiveDaemonCount()} active daemons`);
  }

  /**
   * Initialize console daemon
   */
  private async initializeConsoleDaemon(): Promise<void> {
    try {
      console.log('📝 Initializing Console Daemon...');
      
      this.consoleDaemon = new BrowserConsoleDaemon();
      await this.daemonManager.registerDaemon('console', this.consoleDaemon);
      
      console.log('✅ Console Daemon registered and started');
    } catch (error) {
      console.error('❌ Failed to initialize Console Daemon:', error);
      console.warn('⚠️ Falling back to legacy console implementation');
      this.consoleDaemon = null;
      this.config.enableConsoleDaemon = false;
    }
  }

  /**
   * Get the number of active daemons
   */
  private getActiveDaemonCount(): number {
    return Object.values(this.config).filter(Boolean).length;
  }

  /**
   * Check if console daemon is active
   */
  isConsoleDaemonActive(): boolean {
    return this.config.enableConsoleDaemon === true && this.consoleDaemon !== null;
  }

  /**
   * Get console daemon instance (for direct access if needed)
   */
  getConsoleDaemon(): BrowserConsoleDaemon | null {
    return this.consoleDaemon;
  }

  /**
   * Set session ID for all daemons
   */
  async setSessionId(sessionId: string): Promise<void> {
    if (!this.isInitialized) {
      console.warn('BrowserDaemonController not initialized');
      return;
    }

    const promises: Promise<void>[] = [];

    // Set session for console daemon
    if (this.consoleDaemon) {
      promises.push(
        this.consoleDaemon.handleMessage({
          type: 'console:set_session',
          data: { sessionId },
          timestamp: new Date().toISOString()
        }).then(() => {})
      );
    }

    // Add other daemons as they're implemented...

    await Promise.all(promises);
    
    if (BrowserFeatureFlags.isDebugMode) {
      console.log(`🔗 Session ID set for ${promises.length} active daemons: ${sessionId}`);
    }
  }

  /**
   * Route console capture to appropriate implementation
   */
  async captureConsole(type: string, args: any[]): Promise<boolean> {
    if (this.isConsoleDaemonActive() && this.consoleDaemon) {
      try {
        const response = await this.consoleDaemon.handleMessage({
          type: 'console:capture',
          data: { type, args },
          timestamp: new Date().toISOString()
        });
        return response.success;
      } catch (error) {
        console.warn('Console daemon capture failed, falling back to legacy:', error);
        return false; // Fall back to legacy implementation
      }
    }
    
    return false; // Use legacy implementation
  }

  /**
   * Get status of all daemons
   */
  async getStatus(): Promise<any> {
    const status = {
      isInitialized: this.isInitialized,
      config: this.config,
      activeDaemons: this.getActiveDaemonCount(),
      daemons: {} as any
    };

    if (this.consoleDaemon) {
      const consoleResponse = await this.consoleDaemon.handleMessage({
        type: 'console:get_status',
        data: {},
        timestamp: new Date().toISOString()
      });
      status.daemons.console = consoleResponse.data;
    }

    // Add other daemon statuses as they're implemented...

    return status;
  }

  /**
   * Emergency shutdown of all daemons
   */
  async emergencyShutdown(): Promise<void> {
    console.warn('🚨 Emergency shutdown initiated for browser daemon system');

    try {
      if (this.consoleDaemon) {
        await this.consoleDaemon.handleMessage({
          type: 'console:disable',
          data: {},
          timestamp: new Date().toISOString()
        });
        await this.consoleDaemon.stop();
      }

      // await this.daemonManager.stop(); // TODO: Implement stop method in BrowserDaemonManager
      
      this.isInitialized = false;
      console.log('✅ Emergency shutdown complete');
    } catch (error) {
      console.error('❌ Emergency shutdown failed:', error);
    }
  }

  /**
   * Reload configuration from feature flags
   */
  async reloadConfiguration(): Promise<void> {
    console.log('🔄 Reloading browser daemon configuration...');
    
    await this.emergencyShutdown();
    await this.initialize();
    
    console.log('✅ Configuration reloaded');
  }

  /**
   * Check if any daemons are active
   */
  hasActiveDaemons(): boolean {
    return this.getActiveDaemonCount() > 0;
  }

  /**
   * Get feature flag status
   */
  getFeatureFlagStatus(): any {
    return BrowserFeatureFlags.getStatus();
  }
}

// Export singleton instance for global access
export const browserDaemonController = new BrowserDaemonController();