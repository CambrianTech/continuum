/**
 * Base Daemon - Abstract foundation for all JTAG daemons
 * 
 * Provides common daemon functionality while enforcing proper separation of concerns.
 * All daemons inherit from this to ensure consistent architecture.
 */

import { JTAGModule } from '../../../system/core/shared/JTAGModule';
import type { JTAGContext, JTAGMessage } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter, MessageSubscriber } from '../../../system/core/router/shared/JTAGRouter';

export interface DaemonEntry {
  name: string;
  className: string;
  daemonClass: new (...args: any[]) => DaemonBase;
}

export abstract class DaemonBase extends JTAGModule implements MessageSubscriber {
  public readonly router: JTAGRouter;
  public abstract readonly subpath: string;
  public readonly uuid: string;

  constructor(name: string, context: JTAGContext, router: JTAGRouter) {
    super(name, context);
    this.router = router;
    this.uuid = `${name}_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;
    
    // Initialization will be called explicitly after construction completes
    // to ensure all subclass properties are properly initialized
  }

  /**
   * Initialize daemon - called explicitly after construction to allow subpath access
   */
  async initializeDaemon(): Promise<void> {
    // Register with router using subpath
    this.router.registerSubscriber(this.subpath, this);
    
    // Initialize daemon-specific functionality
    await this.initialize();
  }

  /**
   * Initialize daemon-specific functionality
   * Called automatically after construction
   */
  protected abstract initialize(): Promise<void>;

  /**
   * Handle incoming messages (MessageSubscriber interface)
   * Each daemon implements its own message handling logic
   */
  abstract handleMessage(message: JTAGMessage): Promise<any>;

  /**
   * Get endpoint (MessageSubscriber interface)
   */
  get endpoint(): string {
    return this.subpath;
  }


  /**
   * Cleanup resources when daemon shuts down
   * Override in subclasses if cleanup is needed
   */
  async shutdown(): Promise<void> {
    console.log(`🔄 ${this.toString()}: Shutting down...`);
  }
}