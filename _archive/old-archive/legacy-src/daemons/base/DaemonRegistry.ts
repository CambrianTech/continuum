// ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Daemon Registry - Service discovery for inter-daemon communication
 * 
 * RESPONSIBILITIES:
 * - Register daemons as they start up
 * - Provide discovery mechanism for daemon-to-daemon communication
 * - Track daemon lifecycle and availability
 * - Enable handler registration between daemons
 * 
 * ✅ CLEAN ARCHITECTURE: Uses event-driven discovery pattern
 * ✅ TYPE SAFE: Strong typing for all daemon references
 */

import type { BaseDaemon } from './BaseDaemon';
import { DAEMON_EVENT_BUS } from './DaemonEventBus';
import { SystemEventType } from './EventTypes';

export interface DaemonRegistration {
  daemon: BaseDaemon;
  name: string;
  type: string;
  registeredAt: Date;
  isActive: boolean;
}

export class DaemonRegistry {
  private static instance: DaemonRegistry;
  private registeredDaemons = new Map<string, DaemonRegistration>();
  
  private constructor() {
    // Listen for daemon lifecycle events
    DAEMON_EVENT_BUS.onEvent(SystemEventType.DAEMON_STARTED, (event) => {
      console.log(`📋 Daemon registry noted ${event.daemonName} started`);
    });
    
    DAEMON_EVENT_BUS.onEvent(SystemEventType.DAEMON_STOPPED, (event) => {
      this.unregisterDaemon(event.daemonName);
      console.log(`📋 Daemon registry noted ${event.daemonName} stopped`);
    });
  }
  
  static getInstance(): DaemonRegistry {
    if (!DaemonRegistry.instance) {
      DaemonRegistry.instance = new DaemonRegistry();
    }
    return DaemonRegistry.instance;
  }
  
  /**
   * Register a daemon for discovery
   */
  registerDaemon(daemon: BaseDaemon): void {
    const registration: DaemonRegistration = {
      daemon,
      name: daemon.name,
      type: daemon.daemonType,
      registeredAt: new Date(),
      isActive: true
    };
    
    this.registeredDaemons.set(daemon.name, registration);
    console.log(`📋 Registered daemon: ${daemon.name} (${daemon.daemonType})`);
  }
  
  /**
   * Unregister a daemon
   */
  unregisterDaemon(daemonName: string): void {
    const registration = this.registeredDaemons.get(daemonName);
    if (registration) {
      registration.isActive = false;
      this.registeredDaemons.delete(daemonName);
      console.log(`📋 Unregistered daemon: ${daemonName}`);
    }
  }
  
  /**
   * Find a daemon by name
   */
  findDaemon<T extends BaseDaemon = BaseDaemon>(daemonName: string): T | null {
    const registration = this.registeredDaemons.get(daemonName);
    if (registration?.isActive) {
      return registration.daemon as T;
    }
    return null;
  }
  
  /**
   * Find daemons by type
   */
  findDaemonsByType<T extends BaseDaemon = BaseDaemon>(daemonType: string): T[] {
    const results: T[] = [];
    for (const registration of this.registeredDaemons.values()) {
      if (registration.type === daemonType && registration.isActive) {
        results.push(registration.daemon as T);
      }
    }
    return results;
  }
  
  /**
   * Get all registered daemons
   */
  getAllDaemons(): DaemonRegistration[] {
    return Array.from(this.registeredDaemons.values()).filter(reg => reg.isActive);
  }
  
  /**
   * Check if a daemon is registered
   */
  isDaemonRegistered(daemonName: string): boolean {
    const registration = this.registeredDaemons.get(daemonName);
    return registration?.isActive ?? false;
  }
  
  /**
   * Wait for a daemon to be registered (useful for dependencies)
   */
  async waitForDaemon<T extends BaseDaemon = BaseDaemon>(
    daemonName: string, 
    timeoutMs: number = 5000
  ): Promise<T | null> {
    // Check if already registered
    const existing = this.findDaemon<T>(daemonName);
    if (existing) {
      return existing;
    }
    
    // Wait for registration event
    return new Promise((resolve) => {
      const timeout = setTimeout(() => {
        console.log(`⏰ Timeout waiting for daemon: ${daemonName}`);
        resolve(null);
      }, timeoutMs);
      
      const checkRegistration = (): void => {
        const daemon = this.findDaemon<T>(daemonName);
        if (daemon) {
          clearTimeout(timeout);
          resolve(daemon);
        } else {
          // Check again in 100ms
          setTimeout(checkRegistration, 100);
        }
      };
      
      checkRegistration();
    });
  }
}

// Export singleton instance
export const DAEMON_REGISTRY = DaemonRegistry.getInstance();