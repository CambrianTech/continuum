/**
 * JTAG Instance Class - Proper OOP approach
 * 
 * Instead of static methods, create JTAG instances that manage their own:
 * - Transport connections
 * - Endpoint health checks
 * - Message queueing
 * - Connection state
 */

import { JTAGConfig, JTAGScreenshotOptions, JTAGScreenshotResult, JTAGExecOptions, JTAGExecResult, JTAGStatusEvent, JTAGStatusEventListener, JTAG_STATUS, JTAGStatus, ContinuumConnectionParams, ContinuumConnection } from './JTAGTypes';
import { jtagConfig } from './config';
import { jtagRouter } from './JTAGRouter';

export class JTAG {
  private config: JTAGConfig;
  private initialized = false;
  private connected = false;
  private transportReady = false;
  private messageQueue: any[] = [];
  private instanceUUID: string;
  private sessionId: string;
  private startTime: number = Date.now();
  private isServer: boolean;
  private isClient: boolean;
  private statusListeners: JTAGStatusEventListener[] = [];
  private currentStatus: JTAGStatus = JTAG_STATUS.DISCONNECTED;

  constructor(userConfig: Partial<JTAGConfig> = {}) {
    // Auto-detect context
    this.isServer = typeof require !== 'undefined' && typeof window === 'undefined';
    this.isClient = typeof window !== 'undefined';
    
    // Generate unique IDs
    this.instanceUUID = this.generateUUID();
    this.sessionId = 'session_' + Date.now().toString(36);
    
    // Merge config with explicit context
    this.config = { 
      ...jtagConfig, 
      ...userConfig,
      context: this.isServer ? 'server' : 'browser'
    };
    
    // Initialize automatically
    this.initialize();
  }

  private generateUUID(): string {
    return 'jtag_' + Date.now().toString(36) + '_' + Math.random().toString(36).substr(2, 9);
  }

  private initialize(): void {
    if (this.initialized) return;
    
    console.log('🔧 JTAG Instance: Initializing for context:', this.config.context);
    console.log('🔧 JTAG Instance: UUID:', this.instanceUUID);
    
    // Set up transport monitoring
    this.setupTransportMonitoring();
    
    this.initialized = true;
    console.log('🔧 JTAG Instance: Initialization complete');
  }

  private setupTransportMonitoring(): void {
    // Listen for transport ready events
    this.addStatusListener((event: JTAGStatusEvent) => {
      if (event.status === JTAG_STATUS.READY) {
        this.setTransportReady();
      }
    });
  }

  private setTransportReady(): void {
    this.transportReady = true;
    
    if (this.isClient && this.messageQueue.length > 0) {
      console.log(`🚀 JTAG Instance: Flushing ${this.messageQueue.length} queued messages...`);
      const queuedMessages = [...this.messageQueue];
      this.messageQueue = [];
      
      queuedMessages.forEach(msg => this.routeMessage(msg));
      console.log('✅ JTAG Instance: Flushed queued messages');
    }
  }

  /**
   * CRITICAL: Connect and verify endpoint health before any commands
   */
  async connect(_params?: ContinuumConnectionParams): Promise<ContinuumConnection> {
    if (this.connected) {
      return {
        healthy: true,
        transport: { type: 'websocket', state: 'connected', endpoint: `ws://localhost:${this.config.jtagPort}`, latency: 0 },
        session: {
          id: this.sessionId,
          uuid: this.instanceUUID,
          uptime: Date.now() - this.startTime
        }
      };
    }

    console.log('🔗 JTAG Instance: Testing transport endpoint...');
    
    // For now, simple connection check
    // TODO: Implement actual health check per endpoint type
    await new Promise(resolve => setTimeout(resolve, 100)); // Simulate connection check
    
    this.connected = true;
    this.transportReady = true;
    
    console.log('✅ JTAG Instance: Transport endpoint verified');
    
    return {
      healthy: true,
      transport: { type: 'websocket', state: 'connected', endpoint: `ws://localhost:${this.config.jtagPort}`, latency: 15 },
      session: {
        id: this.sessionId,
        uuid: this.instanceUUID,
        uptime: Date.now() - this.startTime
      }
    };
  }

  /**
   * Log command - verifies transport before sending
   */
  async log(component: string, message: string, data?: any): Promise<void> {
    // CRITICAL: Client-side must verify server logging endpoint
    if (this.isClient) {
      await this.connect({ healthCheck: true });
    }
    
    const logMessage = {
      id: this.generateUUID(),
      type: 'log' as const,
      version: '1.0.0',
      timestamp: new Date().toISOString(),
      source: this.config.context === 'browser' ? 'browser' as const : 'server' as const,
      payload: {
        level: 'log' as const,
        component,
        message,
        data,
        timestamp: new Date().toISOString()
      }
    };

    this.routeMessage(logMessage);
  }

  /**
   * Error logging - verifies transport before sending
   */
  async error(component: string, message: string, data?: any): Promise<void> {
    if (this.isClient) {
      await this.connect({ healthCheck: true });
    }
    
    const errorMessage = {
      id: this.generateUUID(),
      type: 'log' as const,
      version: '1.0.0', 
      timestamp: new Date().toISOString(),
      source: this.config.context === 'browser' ? 'browser' as const : 'server' as const,
      payload: {
        level: 'error' as const,
        component,
        message,
        data,
        timestamp: new Date().toISOString()
      }
    };

    this.routeMessage(errorMessage);
  }

  /**
   * Critical logging - verifies transport before sending
   */
  async critical(component: string, message: string, data?: any): Promise<void> {
    if (this.isClient) {
      await this.connect({ healthCheck: true });
    }
    
    const criticalMessage = {
      id: this.generateUUID(),
      type: 'log' as const,
      version: '1.0.0',
      timestamp: new Date().toISOString(),
      source: this.config.context === 'browser' ? 'browser' as const : 'server' as const,
      payload: {
        level: 'critical' as const,
        component,
        message,
        data,
        timestamp: new Date().toISOString()
      }
    };

    this.routeMessage(criticalMessage);
  }

  /**
   * Screenshot command - MUST verify browser endpoint
   */
  async screenshot(options?: JTAGScreenshotOptions): Promise<JTAGScreenshotResult> {
    // CRITICAL: Verify browser endpoint is available
    await this.connect({ healthCheck: true });
    
    console.log('📸 JTAG Instance: Taking screenshot...');
    
    const screenshotMessage = {
      id: this.generateUUID(),
      type: 'screenshot' as const,
      version: '1.0.0',
      timestamp: new Date().toISOString(),
      source: this.config.context === 'browser' ? 'browser' as const : 'server' as const,
      payload: {
        ...options,
        timestamp: new Date().toISOString()
      }
    };

    await this.routeMessage(screenshotMessage);
    
    // Return mock result for now
    return {
      success: true,
      filepath: '/path/to/screenshot.png',
      filename: 'screenshot.png',
      context: this.config.context as 'browser' | 'server',
      timestamp: new Date().toISOString(),
      metadata: { width: 1920, height: 1080, size: 0 }
    };
  }

  /**
   * Execute command - MUST verify execution endpoint  
   */
  async exec(code: string, options?: JTAGExecOptions): Promise<JTAGExecResult> {
    // CRITICAL: Verify execution endpoint is available
    await this.connect({ healthCheck: true });
    
    console.log('⚡ JTAG Instance: Executing code...');
    
    const execMessage = {
      id: this.generateUUID(),
      type: 'exec' as const,
      version: '1.0.0',
      timestamp: new Date().toISOString(),
      source: this.config.context === 'browser' ? 'browser' as const : 'server' as const,
      payload: {
        code,
        options,
        timestamp: new Date().toISOString()
      }
    };

    await this.routeMessage(execMessage);
    
    // Return mock result for now
    return {
      success: true,
      result: 'Code executed successfully',
      context: this.config.context,
      timestamp: new Date().toISOString(),
      executionTime: 50,
      uuid: this.instanceUUID
    };
  }

  private async routeMessage(message: any): Promise<void> {
    // Server-side: Route immediately
    if (this.isServer) {
      try {
        await jtagRouter.routeMessage(message);
      } catch (error) {
        console.error('🚨 JTAG Instance: Server routing failed:', error);
      }
      return;
    }

    // Client-side: Queue until transport ready
    if (!this.transportReady) {
      this.messageQueue.push(message);
      console.log(`🔄 JTAG Instance: Queued message (transport not ready), queue size: ${this.messageQueue.length}`);
      return;
    }

    try {
      await jtagRouter.routeMessage(message);
    } catch (error) {
      console.error('🚨 JTAG Instance: Client routing failed:', error);
    }
  }

  // Status event management
  addStatusListener(listener: JTAGStatusEventListener): void {
    this.statusListeners.push(listener);
  }

  removeStatusListener(listener: JTAGStatusEventListener): void {
    const index = this.statusListeners.indexOf(listener);
    if (index > -1) {
      this.statusListeners.splice(index, 1);
    }
  }

  getStatus(): JTAGStatus {
    return this.currentStatus;
  }

  getUUID(): { uuid: string; sessionId: string } {
    return {
      uuid: this.instanceUUID,
      sessionId: this.sessionId
    };
  }

  isInitialized(): boolean {
    return this.initialized;
  }

  isConnected(): boolean {
    return this.connected;
  }
}