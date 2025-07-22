/**
 * Universal Message Subscriber Interface
 * 
 * Every daemon component implements this interface to handle messages
 * with typed payloads. Works consistently across client and server contexts.
 */

// Forward declaration to avoid circular dependency
interface JTAGRouter {
  registerSubscriber(endpoint: string, subscriber: MessageSubscriber): void;
}

export interface MessageSubscriber<T = any> {
  /**
   * Handle incoming message with typed payload
   */
  handleMessage(message: DaemonMessage<T>): Promise<DaemonResponse>;
  
  /**
   * Get the endpoint this subscriber handles (without prefix)
   * Router will automatically add /client or /server prefix
   */
  getEndpoint(): string;
  
  /**
   * Get unique UUID for direct access
   */
  getUUID(): string;
  
  /**
   * Register with router (called during daemon initialization)
   */
  registerWithRouter(router: JTAGRouter): Promise<void>;
}

export interface DaemonMessage<T = any> {
  type: string;
  payload: T;
  timestamp?: string;
  uuid?: string;
  sender?: string;
  target?: string;
}

export interface DaemonResponse {
  success: boolean;
  data?: any;
  error?: string;
  timestamp: string;
  forwardTo?: string;
  forwardMessage?: DaemonMessage<any>;
}

/**
 * Base implementation for symmetric daemons
 */
export abstract class BaseDaemon<T = any> implements MessageSubscriber<T> {
  protected endpoint: string;
  protected uuid: string;

  constructor(endpoint: string) {
    this.endpoint = endpoint;
    this.uuid = this.generateUUID();
  }

  abstract handleMessage(message: DaemonMessage<T>): Promise<DaemonResponse>;

  getEndpoint(): string {
    return this.endpoint;
  }

  getUUID(): string {
    return this.uuid;
  }

  async registerWithRouter(router: any): Promise<void> {
    // Default registration - subclasses can override
    router.registerSubscriber(this.endpoint, this);
  }

  protected generateUUID(): string {
    return `daemon_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  protected createResponse(success: boolean, data?: any, error?: string): DaemonResponse {
    return {
      success,
      data,
      error,
      timestamp: new Date().toISOString()
    };
  }
}