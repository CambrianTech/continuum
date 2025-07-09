/**
 * Lifecycle-Aware Continuum Browser API
 * Single global object that manages its own lifecycle and capabilities
 */

import packageJson from '../../package.json';

type ContinuumState = 'initializing' | 'connecting' | 'connected' | 'ready' | 'error';

interface ContinuumAPI {
  readonly version: string;
  readonly state: ContinuumState;
  sessionId: string | null;
  clientId: string | null;
  
  // Core methods
  isConnected(): boolean;
  execute(command: string, params?: any): Promise<any>;
  
  // Dynamic method attachment
  attachMethod(name: string, method: Function): void;
  hasMethod(name: string): boolean;
  
  // Lifecycle events
  onStateChange(callback: (state: ContinuumState) => void): void;
  onReady(callback: () => void): void;
}

class LifecycleContinuumAPI implements ContinuumAPI {
  public readonly version: string;
  public sessionId: string | null = null;
  public clientId: string | null = null;
  
  private _state: ContinuumState = 'initializing';
  private ws: WebSocket | null = null;
  private stateCallbacks: ((state: ContinuumState) => void)[] = [];
  private readyCallbacks: (() => void)[] = [];
  private dynamicMethods: Map<string, Function> = new Map();
  private consoleForwarding = false;
  private consoleMessageQueue: any[] = [];

  constructor() {
    this.version = packageJson.version;
    console.log(`🌐 Continuum v${this.version} lifecycle starting...`);
    
    // Initialize lifecycle
    this.setState('connecting');
    this.initializeConnection();
  }

  get state(): ContinuumState {
    return this._state;
  }

  private setState(newState: ContinuumState): void {
    if (this._state !== newState) {
      console.log(`🔄 Continuum state: ${this._state} → ${newState}`);
      this._state = newState;
      this.stateCallbacks.forEach(callback => callback(newState));
      
      // Handle state-specific logic
      this.handleStateChange(newState);
    }
  }

  private handleStateChange(state: ContinuumState): void {
    switch (state) {
      case 'connected':
        this.enableConsoleForwarding();
        break;
      case 'ready':
        this.readyCallbacks.forEach(callback => callback());
        this.flushConsoleMessageQueue();
        console.log('✅ Continuum API ready for use');
        this.performHealthCheck();
        break;
      case 'error':
        console.error('❌ Continuum API in error state');
        break;
    }
  }

  private initializeConnection(): void {
    try {
      this.ws = new WebSocket('ws://localhost:9000');
      
      this.ws.onopen = () => {
        console.log('🔌 WebSocket connected');
        this.setState('connected');
        
        // Send client init
        this.sendMessage({
          type: 'client_init',
          data: {
            userAgent: navigator.userAgent,
            url: window.location.href,
            timestamp: new Date().toISOString(),
            version: this.version,
            mode: 'join_existing'
          }
        });
      };

      this.ws.onmessage = (event) => {
        this.handleMessage(event);
      };

      this.ws.onclose = () => {
        console.log('🔌 WebSocket disconnected');
        this.setState('error');
      };

      this.ws.onerror = (error) => {
        console.error('🔌 WebSocket error:', error);
        this.setState('error');
      };

    } catch (error) {
      console.error('❌ Failed to initialize WebSocket:', error);
      this.setState('error');
    }
  }

  private handleMessage(event: MessageEvent): void {
    try {
      const message = JSON.parse(event.data);
      
      if (message.type === 'connection_confirmed') {
        this.clientId = message.data?.clientId;
        console.log(`🔌 Client ID: ${this.clientId}`);
        return;
      }

      if (message.type === 'session_ready') {
        this.sessionId = message.data?.sessionId;
        console.log(`🎯 Session: ${this.sessionId}`);
        this.setState('ready');
        return;
      }

      // Handle command responses
      if (message.type === 'execute_command_response') {
        // Emit as custom event for promise handlers
        const event = new CustomEvent('continuum:command_response', { 
          detail: message 
        });
        document.dispatchEvent(event);
        return;
      }

    } catch (error) {
      console.error('❌ Error parsing message:', error);
    }
  }

  private enableConsoleForwarding(): void {
    if (this.consoleForwarding) return;
    
    // Store original console methods
    const originalConsole = {
      log: console.log.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
      info: console.info.bind(console),
      trace: console.trace.bind(console)
    };

    // Override console methods
    console.log = (...args: any[]) => {
      originalConsole.log(...args);
      this.forwardConsole('log', args);
    };

    console.warn = (...args: any[]) => {
      originalConsole.warn(...args);
      this.forwardConsole('warn', args);
    };

    console.error = (...args: any[]) => {
      originalConsole.error(...args);
      this.forwardConsole('error', args);
    };

    console.info = (...args: any[]) => {
      originalConsole.info(...args);
      this.forwardConsole('info', args);
    };

    console.trace = (...args: any[]) => {
      originalConsole.trace(...args);
      this.forwardConsole('trace', args);
    };

    this.consoleForwarding = true;
    console.log('✅ Console forwarding enabled');
  }

  private forwardConsole(type: string, args: any[]): void {
    // Forward as soon as console forwarding is enabled (connected state)
    if (this.state !== 'connected' && this.state !== 'ready') return;

    try {
      const consoleCommand = {
        action: type,
        message: args.join(' '),
        timestamp: new Date().toISOString(),
        sessionId: this.sessionId
      };

      // Only execute if we're in ready state, otherwise queue for later
      if (this.state === 'ready') {
        this.execute('console', consoleCommand).catch(() => {
          // Fail silently to avoid console loops
        });
      } else {
        // Queue message for when we reach ready state
        this.queueConsoleMessage(consoleCommand);
      }
    } catch (error) {
      // Fail silently to avoid console loops
    }
  }

  isConnected(): boolean {
    return this.state === 'ready' && this.ws?.readyState === WebSocket.OPEN;
  }

  async execute(command: string, params: any = {}): Promise<any> {
    if (!this.isConnected()) {
      throw new Error(`Continuum not ready (state: ${this.state})`);
    }

    return new Promise((resolve, reject) => {
      const requestId = `cmd_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      const timeout = setTimeout(() => {
        reject(new Error(`Command '${command}' timed out`));
      }, 10000);

      // Listen for response
      const responseHandler = (event: Event) => {
        const message = (event as CustomEvent).detail;
        if (message.requestId === requestId) {
          clearTimeout(timeout);
          document.removeEventListener('continuum:command_response', responseHandler);
          
          if (message.success) {
            resolve(message.data || { success: true });
          } else {
            reject(new Error(message.error || 'Command failed'));
          }
        }
      };

      document.addEventListener('continuum:command_response', responseHandler);

      // Send command
      this.sendMessage({
        type: 'execute_command',
        data: {
          command,
          params: typeof params === 'string' ? params : JSON.stringify(params),
          requestId,
          sessionId: this.sessionId
        }
      });
    });
  }

  // Dynamic method attachment
  attachMethod(name: string, method: Function): void {
    this.dynamicMethods.set(name, method);
    // Attach to the object dynamically
    (this as any)[name] = method.bind(this);
    console.log(`🔧 Dynamic method attached: ${name}`);
  }

  hasMethod(name: string): boolean {
    return this.dynamicMethods.has(name) || typeof (this as any)[name] === 'function';
  }

  // Lifecycle event handlers
  onStateChange(callback: (state: ContinuumState) => void): void {
    this.stateCallbacks.push(callback);
  }

  onReady(callback: () => void): void {
    if (this.state === 'ready') {
      callback();
    } else {
      this.readyCallbacks.push(callback);
    }
  }

  private queueConsoleMessage(consoleCommand: any): void {
    this.consoleMessageQueue.push(consoleCommand);
  }

  private flushConsoleMessageQueue(): void {
    if (this.consoleMessageQueue.length > 0) {
      console.log(`🔄 Flushing ${this.consoleMessageQueue.length} queued console messages`);
      
      // Send all queued messages
      this.consoleMessageQueue.forEach(command => {
        this.execute('console', command).catch(() => {
          // Fail silently to avoid console loops
        });
      });
      
      // Clear the queue
      this.consoleMessageQueue = [];
    }
  }

  private performHealthCheck(): void {
    console.log('🏥 Performing console forwarding health check...');
    console.error('❌ HEALTH CHECK: Error message test');
    console.warn('⚠️ HEALTH CHECK: Warning message test');
    console.trace('🔍 HEALTH CHECK: Trace message test');
    console.log('✅ HEALTH CHECK: Health check complete');
  }

  private sendMessage(message: any): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({
        ...message,
        timestamp: new Date().toISOString(),
        clientId: this.clientId,
        sessionId: this.sessionId
      }));
    }
  }
}

// Create single global instance
const continuum = new LifecycleContinuumAPI();

// Expose as the ONLY global
(window as any).continuum = continuum;

console.log('🌐 Lifecycle Continuum: Single global instance created');

export default continuum;