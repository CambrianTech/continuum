/**
 * Base Daemon - Abstract foundation for all JTAG daemons
 * 
 * Provides common daemon functionality while enforcing proper separation of concerns.
 * All daemons inherit from this to ensure consistent architecture.
 */

import { JTAGModule } from './JTAGModule';
import { JTAGContext, JTAGMessage } from './JTAGTypes';
import { JTAGRouter, MessageSubscriber } from './JTAGRouter';

export abstract class DaemonBase extends JTAGModule implements MessageSubscriber {
  public readonly router: JTAGRouter;
  public abstract readonly subpath: string;
  public readonly uuid: string;

  constructor(name: string, context: JTAGContext, router: JTAGRouter) {
    super(name, context);
    this.router = router;
    this.uuid = `${name}_${Date.now()}_${Math.random().toString(36).substr(2, 6)}`;
    
    // Registration and initialization will be handled by subclass
    // after subpath is defined
    this.initializeDaemon().catch(error => {
      console.error(`❌ ${this.toString()}: Initialization failed:`, error);
    });
  }

  /**
   * Initialize daemon - called after construction to allow subpath access
   */
  private async initializeDaemon(): Promise<void> {
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