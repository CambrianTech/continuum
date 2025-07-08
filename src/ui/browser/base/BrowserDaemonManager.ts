/**
 * BrowserDaemonManager - Orchestrates browser daemon lifecycle
 * 
 * Mirrors server-side daemon management patterns for consistent
 * architecture across client and server environments.
 * 
 * Responsibilities:
 * - Daemon registration and discovery
 * - Lifecycle management (start/stop/restart)
 * - Message routing between daemons
 * - Health monitoring and diagnostics
 * - Feature flag integration for safe migration
 */

import { BaseBrowserDaemon, BrowserDaemonMessage, BrowserDaemonResponse } from './BaseBrowserDaemon';
import { BrowserDaemonEventBus, getBrowserDaemonEventBus } from './BrowserDaemonEventBus';
import { BrowserFeatureFlags } from '../BrowserFeatureFlags';

export interface BrowserDaemonRegistration {
  name: string;
  instance: BaseBrowserDaemon;
  messageTypes: string[];
  isRunning: boolean;
  version: string;
}

export class BrowserDaemonManager {
  private static instance: BrowserDaemonManager | null = null;
  private daemons = new Map<string, BrowserDaemonRegistration>();
  private eventBus: BrowserDaemonEventBus;
  private isInitialized = false;

  constructor() {
    this.eventBus = getBrowserDaemonEventBus();
  }

  /**
   * Singleton pattern - mirrors server daemon manager
   */
  static getInstance(): BrowserDaemonManager {
    if (!BrowserDaemonManager.instance) {
      BrowserDaemonManager.instance = new BrowserDaemonManager();
    }
    return BrowserDaemonManager.instance;
  }

  /**
   * Initialize daemon manager
   */
  async initialize(): Promise<void> {
    if (this.isInitialized) {
      this.log('Already initialized', 'warn');
      return;
    }

    try {
      this.log('Initializing browser daemon manager...');
      
      // Set up event bus monitoring
      this.setupEventBusMonitoring();
      
      this.isInitialized = true;
      this.log('Browser daemon manager initialized successfully');
      
      // Emit initialization event
      await this.eventBus.emit('daemon_manager:initialized', {
        timestamp: new Date().toISOString()
      }, 'BrowserDaemonManager');
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Failed to initialize: ${errorMessage}`, 'error');
      throw error;
    }
  }

  /**
   * Register a daemon instance
   */
  async registerDaemon(name: string, daemon: BaseBrowserDaemon): Promise<void> {
    if (this.daemons.has(name)) {
      throw new Error(`Daemon '${name}' is already registered`);
    }

    const registration: BrowserDaemonRegistration = {
      name,
      instance: daemon,
      messageTypes: daemon.getMessageTypes(),
      isRunning: false,
      version: daemon.getMetadata().version
    };

    this.daemons.set(name, registration);
    this.log(`Registered daemon: ${name} (${daemon.getMessageTypes().join(', ')})`);

    // Emit registration event
    await this.eventBus.emit('daemon:registered', {
      name,
      messageTypes: registration.messageTypes,
      version: registration.version
    }, 'BrowserDaemonManager');
  }

  /**
   * Start a specific daemon
   */
  async startDaemon(name: string): Promise<void> {
    const registration = this.daemons.get(name);
    if (!registration) {
      throw new Error(`Daemon '${name}' is not registered`);
    }

    if (registration.isRunning) {
      this.log(`Daemon '${name}' is already running`, 'warn');
      return;
    }

    try {
      await registration.instance.start();
      registration.isRunning = true;
      this.log(`Started daemon: ${name}`);

      // Emit start event
      await this.eventBus.emit('daemon:started', {
        name,
        timestamp: new Date().toISOString()
      }, 'BrowserDaemonManager');

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Failed to start daemon '${name}': ${errorMessage}`, 'error');
      throw error;
    }
  }

  /**
   * Stop a specific daemon
   */
  async stopDaemon(name: string): Promise<void> {
    const registration = this.daemons.get(name);
    if (!registration) {
      throw new Error(`Daemon '${name}' is not registered`);
    }

    if (!registration.isRunning) {
      this.log(`Daemon '${name}' is not running`, 'warn');
      return;
    }

    try {
      await registration.instance.stop();
      registration.isRunning = false;
      this.log(`Stopped daemon: ${name}`);

      // Emit stop event
      await this.eventBus.emit('daemon:stopped', {
        name,
        timestamp: new Date().toISOString()
      }, 'BrowserDaemonManager');

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Failed to stop daemon '${name}': ${errorMessage}`, 'error');
      throw error;
    }
  }

  /**
   * Route message to appropriate daemon
   */
  async routeMessage(message: BrowserDaemonMessage): Promise<BrowserDaemonResponse> {
    // Find daemon that handles this message type
    const targetDaemon = this.findDaemonForMessageType(message.type);
    
    if (!targetDaemon) {
      return {
        success: false,
        error: `No daemon registered for message type: ${message.type}`,
        timestamp: new Date().toISOString()
      };
    }

    try {
      const response = await targetDaemon.instance.handleMessage(message);
      
      // Emit message routing event for debugging
      if (BrowserFeatureFlags.isDebugMode) {
        await this.eventBus.emit('message:routed', {
          messageType: message.type,
          targetDaemon: targetDaemon.name,
          success: response.success
        }, 'BrowserDaemonManager');
      }
      
      return response;
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Message routing failed for ${message.type}: ${errorMessage}`, 'error');
      
      return {
        success: false,
        error: `Message handling failed: ${errorMessage}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Get all registered daemons
   */
  getRegisteredDaemons(): BrowserDaemonRegistration[] {
    return Array.from(this.daemons.values());
  }

  /**
   * Get daemon by name
   */
  getDaemon(name: string): BrowserDaemonRegistration | undefined {
    return this.daemons.get(name);
  }

  /**
   * Get system diagnostics
   */
  getDiagnostics() {
    const daemons = this.getRegisteredDaemons();
    
    return {
      isInitialized: this.isInitialized,
      totalDaemons: daemons.length,
      runningDaemons: daemons.filter(d => d.isRunning).length,
      stoppedDaemons: daemons.filter(d => !d.isRunning).length,
      daemons: daemons.map(d => ({
        name: d.name,
        isRunning: d.isRunning,
        version: d.version,
        messageTypes: d.messageTypes
      })),
      eventBus: this.eventBus.getDiagnostics(),
      featureFlags: BrowserFeatureFlags.getStatus()
    };
  }

  /**
   * Emergency shutdown - stop all daemons
   */
  async emergencyShutdown(): Promise<void> {
    this.log('Emergency shutdown initiated', 'warn');
    
    const shutdownPromises = Array.from(this.daemons.values())
      .filter(d => d.isRunning)
      .map(d => this.stopDaemon(d.name).catch(error => {
        this.log(`Emergency stop failed for ${d.name}: ${error}`, 'error');
      }));
      
    await Promise.all(shutdownPromises);
    
    this.log('Emergency shutdown completed');
  }

  /**
   * Find daemon that handles a specific message type
   */
  private findDaemonForMessageType(messageType: string): BrowserDaemonRegistration | null {
    for (const daemon of this.daemons.values()) {
      if (daemon.messageTypes.includes(messageType) && daemon.isRunning) {
        return daemon;
      }
    }
    return null;
  }

  /**
   * Set up event bus monitoring for debugging
   */
  private setupEventBusMonitoring(): void {
    if (BrowserFeatureFlags.isDebugMode) {
      this.eventBus.on('*', (event) => {
        this.log(`Event: ${event.type} from ${event.source}`);
      });
    }
  }

  /**
   * Logging with manager context
   */
  private log(message: string, level: 'info' | 'warn' | 'error' = 'info'): void {
    const timestamp = new Date().toISOString();
    const prefix = `[${timestamp}] [BrowserDaemonManager] ${level.toUpperCase()}:`;
    
    switch (level) {
      case 'error':
        console.error(`${prefix} ${message}`);
        break;
      case 'warn':
        console.warn(`${prefix} ${message}`);
        break;
      default:
        console.log(`${prefix} ${message}`);
        break;
    }
  }
}

/**
 * Convenience function to get the singleton instance
 */
export function getBrowserDaemonManager(): BrowserDaemonManager {
  return BrowserDaemonManager.getInstance();
}