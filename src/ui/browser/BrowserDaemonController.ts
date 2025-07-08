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
import { commandDaemon } from './daemons/CommandDaemon';
import { widgetDaemon } from './daemons/WidgetDaemon';
// import { BrowserWebSocketDaemon } from './websocket/BrowserWebSocketDaemon'; // TODO: Implement WebSocket integration
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
  private commandDaemonEnabled = false;
  private widgetDaemonEnabled = false;
  // private _websocketDaemon: BrowserWebSocketDaemon | null = null; // TODO: Implement WebSocket daemon integration
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

    // Initialize command daemon if enabled
    if (this.config.enableCommandDaemon) {
      await this.initializeCommandDaemon();
    }

    // Initialize widget daemon if enabled
    if (this.config.enableWidgetDaemon) {
      await this.initializeWidgetDaemon();
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
   * Initialize command daemon
   */
  private async initializeCommandDaemon(): Promise<void> {
    try {
      console.log('🎯 Initializing Command Daemon...');
      
      // Command daemon is already a singleton, just mark as enabled
      this.commandDaemonEnabled = true;
      
      console.log('✅ Command Daemon enabled');
    } catch (error) {
      console.error('❌ Failed to initialize Command Daemon:', error);
      console.warn('⚠️ Falling back to legacy command implementation');
      this.commandDaemonEnabled = false;
      this.config.enableCommandDaemon = false;
    }
  }

  /**
   * Initialize widget daemon
   */
  private async initializeWidgetDaemon(): Promise<void> {
    try {
      console.log('🎨 Initializing Widget Daemon...');
      
      await widgetDaemon.initialize();
      this.widgetDaemonEnabled = true;
      
      console.log('✅ Widget Daemon enabled');
    } catch (error) {
      console.error('❌ Failed to initialize Widget Daemon:', error);
      console.warn('⚠️ Falling back to legacy widget implementation');
      this.widgetDaemonEnabled = false;
      this.config.enableWidgetDaemon = false;
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
   * Check if command daemon is active
   */
  isCommandDaemonActive(): boolean {
    return this.config.enableCommandDaemon === true && this.commandDaemonEnabled === true;
  }

  /**
   * Check if widget daemon is active
   */
  isWidgetDaemonActive(): boolean {
    return this.config.enableWidgetDaemon === true && this.widgetDaemonEnabled === true;
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

    // Set session for command daemon
    if (this.isCommandDaemonActive()) {
      commandDaemon.setSessionId(sessionId);
      promises.push(Promise.resolve()); // Synchronous operation, but add to promise count
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
   * Route command execution to appropriate implementation
   */
  async executeCommand(command: string, params: any = {}): Promise<any> {
    if (this.isCommandDaemonActive()) {
      try {
        return await commandDaemon.execute(command, params);
      } catch (error) {
        console.warn('Command daemon execution failed, falling back to legacy:', error);
        throw error; // Re-throw for legacy handling
      }
    }
    
    throw new Error('Command daemon not active - use legacy implementation');
  }

  /**
   * Route widget discovery to appropriate implementation
   */
  async discoverAndLoadWidgets(): Promise<any> {
    if (this.isWidgetDaemonActive()) {
      try {
        return await widgetDaemon.discoverAndLoadWidgets();
      } catch (error) {
        console.warn('Widget daemon discovery failed, falling back to legacy:', error);
        throw error; // Re-throw for legacy handling
      }
    }
    
    throw new Error('Widget daemon not active - use legacy implementation');
  }

  /**
   * Route widget health validation to appropriate implementation
   */
  async validateWidgetHealth(): Promise<any> {
    if (this.isWidgetDaemonActive()) {
      try {
        return await widgetDaemon.validateWidgetHealth();
      } catch (error) {
        console.warn('Widget daemon health validation failed, falling back to legacy:', error);
        throw error; // Re-throw for legacy handling
      }
    }
    
    throw new Error('Widget daemon not active - use legacy implementation');
  }

  /**
   * Handle continuum ready event through widget daemon
   */
  async handleContinuumReady(): Promise<void> {
    if (this.isWidgetDaemonActive()) {
      try {
        await widgetDaemon.handleContinuumReady();
      } catch (error) {
        console.warn('Widget daemon continuum ready handling failed:', error);
        // Don't throw - this is not critical
      }
    }
  }

  /**
   * Initialize command daemon with WebSocket connection
   */
  initializeCommandDaemonConnection(ws: WebSocket, sessionId?: string, clientId?: string): void {
    if (this.isCommandDaemonActive()) {
      commandDaemon.initialize(ws, sessionId, clientId);
    }
  }

  /**
   * Handle command response messages
   */
  handleCommandResponse(message: any): boolean {
    if (this.isCommandDaemonActive()) {
      return commandDaemon.handleCommandResponse(message);
    }
    return false;
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

    if (this.isWidgetDaemonActive()) {
      status.daemons.widget = await widgetDaemon.getWidgetStatus();
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