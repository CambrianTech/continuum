/**
 * Continuum Browser API - TypeScript Implementation
 * 
 * CRITICAL BROWSER INTEGRATION:
 * ============================
 * ASYNC DEPENDENCY TESTING REQUIREMENTS:
 * - WebSocket connection establishment and reconnection handling
 * - Widget readiness event system - widgets wait for API ready
 * - Command execution async chains with proper error handling
 * - Event subscription and unsubscription lifecycle management
 * - Graceful degradation when WebSocket connection fails
 * 
 * API READINESS PATTERN:
 * - window.continuum becomes available
 * - Fires 'continuum:ready' event for widgets to respond
 * - Provides isConnected() status for widget conditional logic
 * - Handles connection state changes with events
 */

interface ContinuumAPI {
  isConnected(): boolean;
  execute(command: string, params?: any): Promise<any>;
  on(event: string, handler: (data: any) => void): void;
  off(event: string, handler?: (data: any) => void): void;
  emit(event: string, data?: any): void;
  connect(wsUrl?: string): Promise<void>;
  disconnect(): void;
  getConnectionState(): 'connecting' | 'connected' | 'disconnected' | 'error';
}

class ContinuumBrowserAPI implements ContinuumAPI {
  private ws: WebSocket | null = null;
  private connectionState: 'connecting' | 'connected' | 'disconnected' | 'error' = 'disconnected';
  private eventHandlers = new Map<string, ((data: any) => void)[]>();
  private messageQueue: any[] = [];
  private clientId: string | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000;

  constructor() {
    console.log('🌐 Continuum Browser API: Initializing...');
  }

  async connect(wsUrl: string = 'ws://localhost:9000'): Promise<void> {
    if (this.connectionState === 'connecting' || this.connectionState === 'connected') {
      return;
    }

    console.log(`🌐 Continuum API: Connecting to ${wsUrl}...`);
    this.connectionState = 'connecting';
    this.emit('continuum:connecting');

    return new Promise((resolve, reject) => {
      try {
        this.ws = new WebSocket(wsUrl);

        const connectionTimeout = setTimeout(() => {
          console.error('🌐 Continuum API: Connection timeout');
          this.connectionState = 'error';
          this.emit('continuum:error', { error: 'Connection timeout' });
          reject(new Error('WebSocket connection timeout'));
        }, 5000);

        this.ws.onopen = () => {
          clearTimeout(connectionTimeout);
          console.log('🌐 Continuum API: WebSocket connection established');
          this.connectionState = 'connected';
          this.reconnectAttempts = 0;
          
          // Send client initialization
          this.sendMessage({
            type: 'client_init',
            data: {
              userAgent: navigator.userAgent,
              url: window.location.href,
              timestamp: new Date().toISOString()
            }
          });

          this.emit('continuum:connected');
          resolve();
        };

        this.ws.onmessage = (event) => {
          this.handleMessage(event);
        };

        this.ws.onclose = (event) => {
          clearTimeout(connectionTimeout);
          console.log(`🌐 Continuum API: Connection closed (${event.code}: ${event.reason})`);
          this.connectionState = 'disconnected';
          this.emit('continuum:disconnected', { code: event.code, reason: event.reason });
          
          // Attempt reconnection
          this.attemptReconnect();
        };

        this.ws.onerror = (error) => {
          clearTimeout(connectionTimeout);
          console.error('🌐 Continuum API: WebSocket error:', error);
          this.connectionState = 'error';
          this.emit('continuum:error', { error });
          reject(error);
        };

      } catch (error) {
        this.connectionState = 'error';
        this.emit('continuum:error', { error });
        reject(error);
      }
    });
  }

  disconnect(): void {
    if (this.ws) {
      console.log('🌐 Continuum API: Disconnecting...');
      this.ws.close(1000, 'Client disconnect');
      this.ws = null;
    }
    this.connectionState = 'disconnected';
    this.clientId = null;
  }

  isConnected(): boolean {
    return this.connectionState === 'connected' && this.ws?.readyState === WebSocket.OPEN;
  }

  getConnectionState(): 'connecting' | 'connected' | 'disconnected' | 'error' {
    return this.connectionState;
  }

  // CLIENT-SIDE HEALTH VALIDATION - Widgets, Commands, and Daemons self-validate
  async validateClientHealth(): Promise<any> {
    console.log('🏥 Starting client-side health validation...');
    
    const healthReport = {
      timestamp: Date.now(),
      environment: 'browser',
      components: [] as any[],
      overall: 'healthy'
    };

    // 1. WEBSOCKET CONNECTION HEALTH
    healthReport.components.push({
      component: 'websocket-connection',
      status: this.connectionState === 'connected' ? 'healthy' : 'failed',
      lastCheck: Date.now(),
      details: `Connection state: ${this.connectionState}`,
      metrics: {
        reconnectAttempts: this.reconnectAttempts,
        readyState: this.ws?.readyState
      }
    });

    // 2. CONTINUUM API AVAILABILITY
    const apiAvailable = typeof window !== 'undefined' && (window as any).continuum;
    healthReport.components.push({
      component: 'continuum-api',
      status: apiAvailable ? 'healthy' : 'failed',
      lastCheck: Date.now(),
      details: apiAvailable ? 'API methods available on window.continuum' : 'API not available',
      metrics: {
        methodsAvailable: apiAvailable ? Object.keys((window as any).continuum).length : 0
      }
    });

    // 3. WIDGET SELF-VALIDATION
    const widgets = ['chat-widget', 'continuum-sidebar'];
    widgets.forEach(widgetName => {
      const widget = document.querySelector(widgetName);
      const widgetStyles = widget ? window.getComputedStyle(widget) : null;
      
      healthReport.components.push({
        component: widgetName,
        status: widget ? 'healthy' : 'failed',
        lastCheck: Date.now(),
        details: widget ? 'Widget element present and styled' : 'Widget element missing from DOM',
        metrics: {
          hasElement: !!widget,
          isVisible: widgetStyles ? widgetStyles.display !== 'none' : false,
          hasStyles: widgetStyles ? widgetStyles.cssText.length > 0 : false
        }
      });
    });

    // 4. SCRIPT LOADING VALIDATION
    const scripts = Array.from(document.querySelectorAll('script[src]'));
    const continuumScript = scripts.find(script => script.getAttribute('src')?.includes('continuum'));
    
    healthReport.components.push({
      component: 'script-loading',
      status: continuumScript ? 'healthy' : 'failed',
      lastCheck: Date.now(),
      details: `${scripts.length} scripts loaded, continuum script: ${continuumScript ? 'found' : 'missing'}`,
      metrics: {
        totalScripts: scripts.length,
        continuumScriptFound: !!continuumScript
      }
    });

    // 5. CONSOLE ERROR MONITORING
    const consoleErrors = (window as any).continuumErrorCount || 0;
    healthReport.components.push({
      component: 'console-errors',
      status: consoleErrors === 0 ? 'healthy' : consoleErrors < 5 ? 'degraded' : 'failed',
      lastCheck: Date.now(),
      details: `${consoleErrors} console errors detected`,
      metrics: {
        errorCount: consoleErrors
      }
    });

    // Calculate overall health
    const failedComponents = healthReport.components.filter(c => c.status === 'failed').length;
    const degradedComponents = healthReport.components.filter(c => c.status === 'degraded').length;
    
    if (failedComponents > 0) {
      healthReport.overall = 'failed';
    } else if (degradedComponents > 0) {
      healthReport.overall = 'degraded';
    } else {
      healthReport.overall = 'healthy';
    }

    // Log comprehensive health report
    console.log('🏥 CLIENT HEALTH REPORT:');
    console.log('========================');
    console.log(`Overall Status: ${healthReport.overall.toUpperCase()}`);
    console.log(`Components Checked: ${healthReport.components.length}`);
    
    healthReport.components.forEach(component => {
      const icon = component.status === 'healthy' ? '✅' : component.status === 'degraded' ? '🟡' : '❌';
      console.log(`${icon} ${component.component}: ${component.status} - ${component.details}`);
      if (component.metrics) {
        console.log(`   Metrics:`, component.metrics);
      }
    });

    console.log('🏥 Health validation complete');
    
    // Forward health report to development portal for JTAG
    try {
      if (this.isConnected()) {
        await this.execute('console', {
          action: 'health_report',
          message: `Client health: ${healthReport.overall} (${healthReport.components.length} components)`,
          source: 'health-validator',
          data: healthReport
        });
      }
    } catch (error) {
      console.log('🏥 Could not forward health report to portal:', error.message);
    }
    
    return healthReport;
  }

  async execute(command: string, params: any = {}): Promise<any> {
    if (!this.isConnected()) {
      throw new Error('Continuum API not connected');
    }

    return new Promise((resolve, reject) => {
      const requestId = `cmd_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      const timeout = setTimeout(() => {
        reject(new Error(`Command '${command}' timed out`));
      }, 10000);

      // Listen for response
      const responseHandler = (data: any) => {
        if (data && data.requestId === requestId) {
          clearTimeout(timeout);
          this.off('command_response', responseHandler);
          
          if (data.result?.success) {
            resolve(data.result);
          } else {
            reject(new Error(data.result?.error || 'Command failed'));
          }
        }
      };

      this.on('command_response', responseHandler);

      // Send command
      this.sendMessage({
        type: 'execute_command',
        data: {
          command,
          params: typeof params === 'string' ? params : JSON.stringify(params),
          requestId
        }
      });
    });
  }

  on(event: string, handler: (data: any) => void): void {
    if (!this.eventHandlers.has(event)) {
      this.eventHandlers.set(event, []);
    }
    this.eventHandlers.get(event)!.push(handler);
  }

  off(event: string, handler?: (data: any) => void): void {
    const handlers = this.eventHandlers.get(event);
    if (!handlers) return;

    if (handler) {
      const index = handlers.indexOf(handler);
      if (index > -1) {
        handlers.splice(index, 1);
      }
    } else {
      // Remove all handlers for this event
      this.eventHandlers.set(event, []);
    }
  }

  emit(event: string, data?: any): void {
    const handlers = this.eventHandlers.get(event);
    if (handlers) {
      handlers.forEach(handler => {
        try {
          handler(data);
        } catch (error) {
          console.error(`🌐 Continuum API: Error in event handler for '${event}':`, error);
        }
      });
    }

    // Also emit as DOM event for widgets that prefer DOM events
    const domEvent = new CustomEvent(event, { detail: data });
    document.dispatchEvent(domEvent);
  }

  private sendMessage(message: any): void {
    if (this.isConnected() && this.ws) {
      this.ws.send(JSON.stringify({
        ...message,
        timestamp: new Date().toISOString(),
        clientId: this.clientId
      }));
    } else {
      // Queue message for when connection is ready
      this.messageQueue.push(message);
    }
  }

  private handleMessage(event: MessageEvent): void {
    try {
      const message = JSON.parse(event.data);
      
      // Handle connection confirmation
      if (message.type === 'connection_confirmed') {
        this.clientId = message.data?.clientId;
        console.log(`🌐 Continuum API: Client ID assigned: ${this.clientId}`);
        
        // Send queued messages
        this.flushMessageQueue();
        return;
      }

      // Handle command responses
      if (message.type === 'execute_command_response') {
        this.emit('command_response', message.data);
        return;
      }

      // Handle other message types - only emit as command_response if it has requestId
      if (message.data && message.data.requestId) {
        this.emit('command_response', message.data);
      } else {
        // Handle as regular event
        const eventName = message.type.replace(/_response$/, '');
        this.emit(eventName, message.data);
      }
      
    } catch (error) {
      console.error('🌐 Continuum API: Error parsing message:', error);
    }
  }

  private flushMessageQueue(): void {
    while (this.messageQueue.length > 0 && this.isConnected()) {
      const message = this.messageQueue.shift();
      this.sendMessage(message);
    }
  }

  private attemptReconnect(): void {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      console.error('🌐 Continuum API: Max reconnection attempts reached');
      this.connectionState = 'error';
      this.emit('continuum:max_reconnect_attempts');
      return;
    }

    const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts);
    this.reconnectAttempts++;
    
    console.log(`🌐 Continuum API: Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`);
    
    setTimeout(() => {
      if (this.connectionState === 'disconnected') {
        this.connect().catch(error => {
          console.error('🌐 Continuum API: Reconnection failed:', error);
        });
      }
    }, delay);
  }
}

// Initialize and expose global API
console.log('🌐 Continuum Browser API: Creating global instance...');
const continuum = new ContinuumBrowserAPI();

// Expose to window for widgets
(window as any).continuum = continuum;

// Auto-connect on load
continuum.connect().then(async () => {
  console.log('🌐 Continuum API: Ready! Widgets can now connect.');
  
  // Fire ready event for widgets to respond
  continuum.emit('continuum:ready');
  
  // Run automatic client-side health validation
  try {
    const healthReport = await continuum.validateClientHealth();
    
    // Report health back to server
    if (continuum.isConnected()) {
      await continuum.execute('health', { 
        clientReport: healthReport,
        source: 'browser-auto-validation'
      });
    }
  } catch (error) {
    console.error('🏥 Auto health validation failed:', error);
  }
  
}).catch(async (error) => {
  console.error('🌐 Continuum API: Initial connection failed:', error);
  
  // Still fire ready event so widgets can handle disconnected state
  continuum.emit('continuum:ready');
  
  // Run health validation even if connection failed
  try {
    await continuum.validateClientHealth();
  } catch (healthError) {
    console.error('🏥 Health validation failed:', healthError);
  }
});

export default continuum;