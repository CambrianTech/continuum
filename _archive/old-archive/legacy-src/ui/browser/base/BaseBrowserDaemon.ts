/**
 * BaseBrowserDaemon - Foundation for browser-side daemon architecture
 * 
 * Mirrors server-side BaseDaemon patterns for consistent architecture
 * across client and server environments.
 * 
 * Key Design Principles:
 * - Event-driven communication via BrowserDaemonEventBus
 * - Message type declarations for routing
 * - Lifecycle management (start/stop)
 * - Logging and debugging capabilities
 * - Isolated, testable modules
 */

export interface BrowserDaemonMessage {
  type: string;
  data: any;
  requestId?: string;
  timestamp: string;
}

export interface BrowserDaemonResponse {
  success: boolean;
  data?: any;
  error?: string;
  timestamp: string;
}

export abstract class BaseBrowserDaemon {
  protected name: string;
  protected version: string;
  protected isRunning = false;
  
  constructor(name: string, version: string = '1.0.0') {
    this.name = name;
    this.version = version;
  }

  /**
   * Get message types this daemon handles
   * Mirrors server daemon getMessageTypes() pattern
   */
  abstract getMessageTypes(): string[];

  /**
   * Handle incoming messages
   * Mirrors server daemon handleMessage() pattern
   */
  abstract handleMessage(message: BrowserDaemonMessage): Promise<BrowserDaemonResponse>;

  /**
   * Start daemon - override for initialization logic
   */
  protected async onStart(): Promise<void> {
    // Override in subclasses
  }

  /**
   * Stop daemon - override for cleanup logic
   */
  protected async onStop(): Promise<void> {
    // Override in subclasses
  }

  /**
   * Public start method
   */
  async start(): Promise<void> {
    if (this.isRunning) {
      this.log('Already running', 'warn');
      return;
    }

    try {
      await this.onStart();
      this.isRunning = true;
      this.log('Started successfully');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Failed to start: ${errorMessage}`, 'error');
      throw error;
    }
  }

  /**
   * Public stop method
   */
  async stop(): Promise<void> {
    if (!this.isRunning) {
      this.log('Already stopped', 'warn');
      return;
    }

    try {
      await this.onStop();
      this.isRunning = false;
      this.log('Stopped successfully');
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Failed to stop: ${errorMessage}`, 'error');
      throw error;
    }
  }

  /**
   * Check if daemon is running
   */
  getIsRunning(): boolean {
    return this.isRunning;
  }

  /**
   * Get daemon metadata
   */
  getMetadata() {
    return {
      name: this.name,
      version: this.version,
      isRunning: this.isRunning,
      messageTypes: this.getMessageTypes()
    };
  }

  /**
   * Logging with daemon context
   * Mirrors server daemon logging patterns
   */
  protected log(message: string, level: 'info' | 'warn' | 'error' = 'info'): void {
    const timestamp = new Date().toISOString();
    const prefix = `[${timestamp}] [${this.name}] ${level.toUpperCase()}:`;
    
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

  /**
   * Emit events to other daemons
   * Will be connected to BrowserDaemonEventBus
   */
  protected emit(eventType: string, _data?: any): void {
    // TODO: Connect to BrowserDaemonEventBus
    this.log(`Event emitted: ${eventType}`, 'info');
  }

  /**
   * Send message to another daemon
   * Mirrors server daemon messaging patterns
   */
  protected async sendMessage(
    targetDaemon: string, 
    messageType: string, 
    _data: any
  ): Promise<BrowserDaemonResponse> {
    // TODO: Connect to BrowserDaemonManager for routing
    this.log(`Message sent to ${targetDaemon}: ${messageType}`, 'info');
    
    return {
      success: true,
      data: { message: 'Message routing not yet implemented' },
      timestamp: new Date().toISOString()
    };
  }

  /**
   * Create standardized response
   */
  protected createResponse(success: boolean, data?: any, error?: string): BrowserDaemonResponse {
    const response: BrowserDaemonResponse = {
      success,
      timestamp: new Date().toISOString()
    };
    
    if (data !== undefined) {
      response.data = data;
    }
    
    if (error !== undefined) {
      response.error = error;
    }
    
    return response;
  }

  /**
   * Create error response
   */
  protected createErrorResponse(error: string): BrowserDaemonResponse {
    return this.createResponse(false, undefined, error);
  }

  /**
   * Create success response
   */
  protected createSuccessResponse(data?: any): BrowserDaemonResponse {
    return this.createResponse(true, data);
  }
}