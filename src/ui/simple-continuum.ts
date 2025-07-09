/**
 * Simple Continuum Browser Object
 * Provides basic window.continuum interface without auto-connecting
 * Waits for CLI to establish session, then provides API
 */

import packageJson from '../../package.json';

interface SimpleContinuumAPI {
  readonly version: string;
  isConnected(): boolean;
  execute(command: string, params?: any): Promise<any>;
  sessionId: string | null;
  clientId: string | null;
}

class SimpleContinuumBrowserAPI implements SimpleContinuumAPI {
  public readonly version: string;
  private ws: WebSocket | null = null;
  public sessionId: string | null = null;
  public clientId: string | null = null;
  private isReady = false;

  constructor() {
    this.version = packageJson.version;
    console.log(`🌐 Simple Continuum v${this.version}: Waiting for CLI session...`);
    
    // Set up console forwarding immediately
    this.setupConsoleForwarding();
    
    // Connect to existing WebSocket server (CLI should have created session)
    this.connectToExistingSession();
  }

  private connectToExistingSession(): void {
    // Semaphore: Check if CLI is already establishing session
    const sessionSemaphore = sessionStorage.getItem('continuum-session-establishing');
    if (sessionSemaphore) {
      console.log('🔒 CLI session establishment in progress - waiting...');
      // Wait for CLI to finish, then connect
      setTimeout(() => this.connectToExistingSession(), 500);
      return;
    }

    try {
      this.ws = new WebSocket('ws://localhost:9000');
      
      this.ws.onopen = () => {
        console.log('🔌 Connected to existing WebSocket server');
        
        // Check if session already exists before sending any messages
        this.checkForExistingSession().then(hasSession => {
          if (hasSession) {
            console.log('✅ Using existing CLI session');
          } else {
            console.log('🔄 No existing session found - waiting for CLI');
          }
          
          // Send client init but don't create session
          this.sendMessage({
            type: 'client_init',
            data: {
              userAgent: navigator.userAgent,
              url: window.location.href,
              timestamp: new Date().toISOString(),
              version: this.version,
              mode: 'join_existing' // Signal we want to join, not create
            }
          });
        });
      };

      this.ws.onmessage = (event) => {
        this.handleMessage(event);
      };

      this.ws.onclose = () => {
        console.log('🔌 WebSocket disconnected');
      };

      this.ws.onerror = (error) => {
        console.error('🔌 WebSocket error:', error);
      };

    } catch (error) {
      console.error('❌ Failed to connect to WebSocket:', error);
    }
  }

  private setupConsoleForwarding(): void {
    // Store original console methods
    const originalConsole = {
      log: console.log.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
      info: console.info.bind(console)
    };

    // Override console methods to forward to server
    console.log = (...args: any[]) => {
      originalConsole.log(...args);  // Show in browser
      this.forwardConsoleMessage('log', args);
    };

    console.warn = (...args: any[]) => {
      originalConsole.warn(...args);
      this.forwardConsoleMessage('warn', args);
    };

    console.error = (...args: any[]) => {
      originalConsole.error(...args);
      this.forwardConsoleMessage('error', args);
    };

    console.info = (...args: any[]) => {
      originalConsole.info(...args);
      this.forwardConsoleMessage('info', args);
    };

    console.log('✅ Console forwarding enabled');
  }

  private forwardConsoleMessage(type: string, args: any[]): void {
    if (!this.isConnected()) {
      return; // Don't forward if not connected
    }

    try {
      const consoleCommand = {
        action: type,
        message: args.join(' '),
        timestamp: new Date().toISOString(),
        sessionId: this.sessionId
      };

      // Execute console command
      this.execute('console', consoleCommand).catch(() => {
        // Fail silently to avoid console loops
      });
    } catch (error) {
      // Fail silently to avoid console loops
    }
  }

  private async checkForExistingSession(): Promise<boolean> {
    try {
      // Quick HTTP check to see if a session already exists
      const response = await fetch('/api/session/current', { 
        method: 'GET',
        headers: { 'Accept': 'application/json' }
      });
      
      if (response.ok) {
        const data = await response.json();
        return !!(data.sessionId || data.currentSession);
      }
    } catch (error) {
      // If check fails, assume no session exists
      console.log('🔍 Session check failed - assuming no existing session');
    }
    return false;
  }

  private handleMessage(event: MessageEvent): void {
    try {
      const message = JSON.parse(event.data);
      
      // Handle connection confirmation
      if (message.type === 'connection_confirmed') {
        this.clientId = message.data?.clientId;
        console.log(`🔌 Client ID assigned: ${this.clientId}`);
        return;
      }

      // Handle session_ready message - this comes from CLI
      if (message.type === 'session_ready') {
        this.sessionId = message.data?.sessionId;
        this.isReady = true;
        console.log(`🎯 Session ready: ${this.sessionId}`);
        console.log('✅ Simple Continuum API ready for use');
        
        // Fire ready event
        const readyEvent = new CustomEvent('continuum:ready', { 
          detail: { sessionId: this.sessionId, clientId: this.clientId } 
        });
        document.dispatchEvent(readyEvent);
        return;
      }

    } catch (error) {
      console.error('❌ Error parsing WebSocket message:', error);
    }
  }

  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN && this.isReady;
  }

  async execute(command: string, params: any = {}): Promise<any> {
    if (!this.isConnected()) {
      throw new Error('Continuum not ready - waiting for CLI session');
    }

    return new Promise((resolve, reject) => {
      const requestId = `cmd_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      const timeout = setTimeout(() => {
        reject(new Error(`Command '${command}' timed out`));
      }, 10000);

      // Listen for response (simple approach)
      const responseHandler = (event: MessageEvent) => {
        try {
          const message = JSON.parse(event.data);
          if (message.type === 'execute_command_response' && message.requestId === requestId) {
            clearTimeout(timeout);
            this.ws?.removeEventListener('message', responseHandler);
            
            if (message.success) {
              resolve(message.data || { success: true });
            } else {
              reject(new Error(message.error || 'Command failed'));
            }
          }
        } catch (error) {
          // Ignore parse errors for other messages
        }
      };

      this.ws?.addEventListener('message', responseHandler);

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

// Create and expose simple global API  
const continuum = new SimpleContinuumBrowserAPI();

// Expose to window
(window as any).continuum = continuum;

console.log('🌐 Simple Continuum: Waiting for CLI to establish session...');

export default continuum;