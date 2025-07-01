/**
 * BaseDaemon - Universal Foundation for All Daemons
 * 
 * ARCHITECTURE:
 * - Consistent lifecycle: start → ready → process requests → stop
 * - Message-based communication (Web Worker compatible)
 * - Handler registration system for different request types
 * - Built-in logging, error handling, and health monitoring
 * - Works in both Web Worker and main thread contexts
 */

export interface DaemonRequest {
  id?: string;
  type: string;
  timestamp?: number;
  [key: string]: any;
}

export interface DaemonResponse {
  id?: string;
  success: boolean;
  result?: any;
  error?: string;
  timestamp: number;
  processingTime?: number;
}

export interface DaemonStatus {
  name: string;
  status: 'starting' | 'ready' | 'processing' | 'stopping' | 'stopped' | 'error';
  uptime: number;
  requestsProcessed: number;
  lastActivity: number;
  version: string;
}

/**
 * BaseDaemon - Foundation for all daemon implementations
 */
export abstract class BaseDaemon {
  protected name: string;
  protected version: string = '1.0.0';
  protected status: DaemonStatus['status'] = 'stopped';
  protected startTime: number = 0;
  protected requestsProcessed: number = 0;
  protected lastActivity: number = 0;
  
  private handlers: Map<string, Function> = new Map();
  private isWebWorker: boolean;

  constructor(name: string) {
    this.name = name;
    this.isWebWorker = typeof self !== 'undefined' && typeof window === 'undefined';
    
    if (this.isWebWorker) {
      this.setupWebWorkerCommunication();
    }
  }

  /**
   * Start the daemon
   */
  async start(): Promise<void> {
    try {
      this.status = 'starting';
      this.startTime = Date.now();
      this.log('Starting daemon...');

      await this.onStart();
      
      this.status = 'ready';
      this.log('Daemon ready');
      
      this.emit('daemon:ready', this.getStatus());
    } catch (error) {
      this.status = 'error';
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Failed to start: ${errorMessage}`, 'error');
      throw error;
    }
  }

  /**
   * Stop the daemon
   */
  async stop(): Promise<void> {
    try {
      this.status = 'stopping';
      this.log('Stopping daemon...');

      await this.onStop();
      
      this.status = 'stopped';
      this.log('Daemon stopped');
      
      this.emit('daemon:stopped', this.getStatus());
    } catch (error) {
      this.status = 'error';
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Failed to stop cleanly: ${errorMessage}`, 'error');
      throw error;
    }
  }

  /**
   * Process a request
   */
  async processRequest(request: DaemonRequest): Promise<DaemonResponse> {
    const startTime = Date.now();
    const requestId = request.id || this.generateRequestId();
    
    try {
      this.status = 'processing';
      this.requestsProcessed++;
      this.lastActivity = startTime;

      this.log(`Processing request: ${request.type}`, 'debug');

      const handler = this.handlers.get(request.type);
      if (!handler) {
        throw new Error(`No handler registered for request type: ${request.type}`);
      }

      const result = await handler(request);
      const processingTime = Date.now() - startTime;

      this.status = 'ready';
      this.log(`Request completed in ${processingTime}ms`, 'debug');

      return {
        id: requestId,
        success: true,
        result,
        timestamp: Date.now(),
        processingTime
      };

    } catch (error) {
      const processingTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      this.status = 'ready'; // Return to ready state after error
      this.log(`Request failed: ${errorMessage}`, 'error');

      return {
        id: requestId,
        success: false,
        error: errorMessage,
        timestamp: Date.now(),
        processingTime
      };
    }
  }

  /**
   * Register request handler
   */
  protected registerHandler(type: string, handler: Function): void {
    this.handlers.set(type, handler);
    this.log(`Registered handler for: ${type}`, 'debug');
  }

  /**
   * Register multiple handlers
   */
  protected registerHandlers(handlers: { [type: string]: Function }): void {
    for (const [type, handler] of Object.entries(handlers)) {
      this.registerHandler(type, handler);
    }
  }

  /**
   * Get daemon status
   */
  getStatus(): DaemonStatus {
    return {
      name: this.name,
      status: this.status,
      uptime: this.startTime > 0 ? Date.now() - this.startTime : 0,
      requestsProcessed: this.requestsProcessed,
      lastActivity: this.lastActivity,
      version: this.version
    };
  }

  /**
   * Health check
   */
  async healthCheck(): Promise<boolean> {
    try {
      const isHealthy = await this.onHealthCheck();
      this.log(`Health check: ${isHealthy ? 'healthy' : 'unhealthy'}`, 'debug');
      return isHealthy;
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`Health check failed: ${errorMessage}`, 'error');
      return false;
    }
  }

  /**
   * Logging with context
   */
  protected log(message: string, level: 'debug' | 'info' | 'warn' | 'error' = 'info'): void {
    const timestamp = new Date().toISOString();
    const logMessage = `[${timestamp}] [${this.name}] ${level.toUpperCase()}: ${message}`;
    
    if (this.isWebWorker) {
      // Send log to main thread
      self.postMessage({
        type: 'daemon:log',
        level,
        message: logMessage,
        daemon: this.name,
        timestamp
      });
    } else {
      // Log directly to console
      console[level](logMessage);
    }
  }

  /**
   * Emit events
   */
  protected emit(eventType: string, data?: any): void {
    if (this.isWebWorker) {
      self.postMessage({
        type: 'daemon:event',
        eventType,
        data,
        daemon: this.name,
        timestamp: Date.now()
      });
    } else {
      // Dispatch custom event (if in main thread)
      if (typeof document !== 'undefined') {
        document.dispatchEvent(new CustomEvent(eventType, { detail: data }));
      }
    }
  }

  /**
   * Generate unique request ID
   */
  private generateRequestId(): string {
    return `${this.name}_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  /**
   * Setup Web Worker communication
   */
  private setupWebWorkerCommunication(): void {
    self.onmessage = async (event: MessageEvent) => {
      const { type, ...request } = event.data;

      try {
        switch (type) {
          case 'daemon:start':
            await this.start();
            break;

          case 'daemon:stop':
            await this.stop();
            break;

          case 'daemon:status':
            self.postMessage({
              type: 'daemon:status:response',
              status: this.getStatus()
            });
            break;

          case 'daemon:health':
            const isHealthy = await this.healthCheck();
            self.postMessage({
              type: 'daemon:health:response',
              healthy: isHealthy,
              status: this.getStatus()
            });
            break;

          default:
            // Handle as regular request
            const response = await this.processRequest({ type, ...request });
            self.postMessage({
              type: 'daemon:response',
              ...response
            });
            break;
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        self.postMessage({
          type: 'daemon:error',
          error: errorMessage,
          originalRequest: event.data
        });
      }
    };

    // Auto-start daemon in Web Worker
    this.start().catch(error => {
      const errorMessage = error instanceof Error ? error.message : String(error);
      self.postMessage({
        type: 'daemon:startup:error',
        error: errorMessage
      });
    });
  }

  // Abstract methods that subclasses must implement
  protected abstract onStart(): Promise<void>;
  protected abstract onStop(): Promise<void>;
  
  protected async onHealthCheck(): Promise<boolean> {
    // Default implementation - override in subclasses
    return this.status === 'ready' || this.status === 'processing';
  }
}

/**
 * DaemonManager - Manages daemon instances (for main thread)
 */
export class DaemonManager {
  private daemons: Map<string, Worker | BaseDaemon> = new Map();
  // private messageHandlers: Map<string, Function> = new Map(); // Unused - TODO: implement if needed

  /**
   * Start a daemon (Web Worker or direct instance)
   */
  async startDaemon(name: string, workerScript?: string, DaemonClass?: new (name: string) => BaseDaemon): Promise<void> {
    if (this.daemons.has(name)) {
      throw new Error(`Daemon ${name} is already running`);
    }

    if (workerScript) {
      // Start as Web Worker
      const worker = new Worker(workerScript);
      this.setupWorkerEventHandlers(name, worker);
      this.daemons.set(name, worker);
      
      // Send start command
      worker.postMessage({ type: 'daemon:start' });
    } else if (DaemonClass) {
      // Start as direct instance
      const daemon = new DaemonClass(name);
      this.daemons.set(name, daemon);
      await daemon.start();
    } else {
      throw new Error('Must provide either workerScript or DaemonClass');
    }
  }

  /**
   * Stop a daemon
   */
  async stopDaemon(name: string): Promise<void> {
    const daemon = this.daemons.get(name);
    if (!daemon) {
      throw new Error(`Daemon ${name} not found`);
    }

    if (daemon instanceof Worker) {
      daemon.postMessage({ type: 'daemon:stop' });
      daemon.terminate();
    } else {
      await daemon.stop();
    }

    this.daemons.delete(name);
  }

  /**
   * Send request to daemon
   */
  async sendRequest(daemonName: string, request: DaemonRequest): Promise<DaemonResponse> {
    const daemon = this.daemons.get(daemonName);
    if (!daemon) {
      throw new Error(`Daemon ${daemonName} not found`);
    }

    if (daemon instanceof Worker) {
      return new Promise((resolve, reject) => {
        const requestId = request.id || `req_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
        
        const timeout = setTimeout(() => {
          reject(new Error(`Request to ${daemonName} timed out`));
        }, 10000);

        const handler = (event: MessageEvent) => {
          if (event.data.type === 'daemon:response' && event.data.id === requestId) {
            clearTimeout(timeout);
            daemon.removeEventListener('message', handler);
            resolve(event.data);
          }
        };

        daemon.addEventListener('message', handler);
        daemon.postMessage({ ...request, id: requestId });
      });
    } else {
      return await daemon.processRequest(request);
    }
  }

  /**
   * Get daemon status
   */
  async getDaemonStatus(name: string): Promise<DaemonStatus | null> {
    const daemon = this.daemons.get(name);
    if (!daemon) return null;

    if (daemon instanceof Worker) {
      return new Promise((resolve) => {
        const handler = (event: MessageEvent) => {
          if (event.data.type === 'daemon:status:response') {
            daemon.removeEventListener('message', handler);
            resolve(event.data.status);
          }
        };
        
        daemon.addEventListener('message', handler);
        daemon.postMessage({ type: 'daemon:status' });
      });
    } else {
      return daemon.getStatus();
    }
  }

  /**
   * Setup event handlers for Web Worker daemon
   */
  private setupWorkerEventHandlers(name: string, worker: Worker): void {
    worker.addEventListener('message', (event) => {
      const { type, ...data } = event.data;
      
      switch (type) {
        case 'daemon:log':
          (console as any)[data.level](data.message);
          break;
          
        case 'daemon:event':
          if (typeof document !== 'undefined') {
            document.dispatchEvent(new CustomEvent(data.eventType, { detail: data.data }));
          }
          break;
          
        case 'daemon:error':
          console.error(`Daemon ${name} error:`, data.error);
          break;
      }
    });

    worker.addEventListener('error', (error) => {
      console.error(`Daemon ${name} worker error:`, error);
    });
  }
}