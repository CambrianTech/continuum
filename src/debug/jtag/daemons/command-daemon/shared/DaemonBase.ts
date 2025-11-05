/**
 * Base Daemon - Abstract foundation for all JTAG daemons
 * 
 * Provides common daemon functionality while enforcing proper separation of concerns.
 * All daemons inherit from this to ensure consistent architecture.
 */

import { JTAGModule } from '../../../system/core/shared/JTAGModule';
import type { JTAGEnvironment, JTAGContext, JTAGMessage, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import {JTAGMessageFactory, useEnvironment} from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter, MessageSubscriber } from '../../../system/core/router/shared/JTAGRouter';
import { type RouterResult } from '../../../system/core/router/shared/RouterTypes';
import { type BaseResponsePayload } from '../../../system/core/types/ResponseTypes';

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
  abstract handleMessage(message: JTAGMessage): Promise<BaseResponsePayload>;

  /**
   * Get endpoint (MessageSubscriber interface)
   */
  get endpoint(): string {
    return this.subpath;
  }


  protected createRequestMessage(endpoint:string, payload: JTAGPayload): JTAGMessage {
    return JTAGMessageFactory.createRequest(
      this.context,
      this.subpath,
      endpoint,
      payload
    );
  }


  /**
   * Execute remote daemon operation - universal cross-environment method
   * Allows any daemon to call operations on any other daemon
   */
  protected async executeRemote(message: JTAGMessage, environment: JTAGEnvironment): Promise<RouterResult> { 
    message.endpoint = useEnvironment(message.endpoint, environment);
    console.log(`⚡ ${this.toString()}: Executing remote operation: ${message.endpoint}`);
    return this.router.postMessage(message);
  }
  
  /**
   * Cleanup resources when daemon shuts down
   * Override in subclasses if cleanup is needed
   */
  async shutdown(): Promise<void> {
    console.log(`🔄 ${this.toString()}: Shutting down...`);
  }
}