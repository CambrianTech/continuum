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
  readonly version: string;
  isConnected(): boolean;
  execute(command: string, params?: any): Promise<any>;
  on(event: string, handler: (data: any) => void): void;
  off(event: string, handler?: (data: any) => void): void;
  emit(event: string, data?: any): void;
  connect(wsUrl?: string): Promise<void>;
  disconnect(): void;
  getConnectionState(): 'connecting' | 'connected' | 'disconnected' | 'error';
  
  // Widget discovery methods
  discoverAndLoadWidgets(): Promise<void>;
}

class ContinuumBrowserAPI implements ContinuumAPI {
  public readonly version: string;
  private ws: WebSocket | null = null;
  private connectionState: 'connecting' | 'connected' | 'disconnected' | 'error' = 'disconnected';
  private eventHandlers = new Map<string, ((data: any) => void)[]>();
  private messageQueue: any[] = [];
  private clientId: string | null = null;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000;

  constructor() {
    this.version = (window as any).__CONTINUUM_VERSION__ || 'unknown';
    console.log(`🌐 Continuum Browser API v${this.version}: Initializing...`);
    this.setupConsoleErrorCapture();
  }

  private setupConsoleErrorCapture(): void {
    // Capture ALL console logs in real-time and forward to development portal
    const originalConsole = {
      log: console.log,
      info: console.info,
      warn: console.warn,
      error: console.error,
      debug: console.debug,
      trace: console.trace,
      table: console.table,
      group: console.group,
      groupEnd: console.groupEnd
    };
    
    let errorCount = 0;

    // Override ALL console methods to capture everything
    console.log = (...args: any[]) => {
      const timestamp = `[${(Date.now() % 100000)/1000}s]`;
      originalConsole.log.apply(console, [timestamp, ...args]);
      this.forwardConsoleLog('log', args);
    };

    console.info = (...args: any[]) => {
      originalConsole.info.apply(console, args);
      this.forwardConsoleLog('info', args);
    };

    console.warn = (...args: any[]) => {
      const timestamp = `[${(Date.now() % 100000)/1000}s]`;
      originalConsole.warn.apply(console, [timestamp, ...args]);
      this.forwardConsoleLog('warn', args);
    };

    console.error = (...args: any[]) => {
      const timestamp = `[${(Date.now() % 100000)/1000}s]`;
      originalConsole.error.apply(console, [timestamp, ...args]);
      errorCount++;
      (window as any).continuumErrorCount = errorCount;
      this.forwardConsoleLog('error', args);
    };

    console.debug = (...args: any[]) => {
      originalConsole.debug.apply(console, args);
      this.forwardConsoleLog('debug', args);
    };

    console.trace = (...args: any[]) => {
      originalConsole.trace.apply(console, args);
      this.forwardConsoleLog('trace', args);
    };

    console.table = (...args: any[]) => {
      (originalConsole.table as any).apply(console, args);
      this.forwardConsoleLog('table', args);
    };

    console.group = (...args: any[]) => {
      originalConsole.group.apply(console, args);
      this.forwardConsoleLog('group', args);
    };

    console.groupEnd = () => {
      originalConsole.groupEnd.apply(console);
      this.forwardConsoleLog('groupEnd', []);
    };

    // Capture unhandled errors
    window.addEventListener('error', (event) => {
      errorCount++;
      (window as any).continuumErrorCount = errorCount;
      
      this.forwardConsoleLog('unhandled', [{
        message: event.message,
        filename: event.filename,
        lineno: event.lineno,
        colno: event.colno,
        error: event.error?.stack || event.error
      }]);
    });

    // Capture unhandled promise rejections
    window.addEventListener('unhandledrejection', (event) => {
      errorCount++;
      (window as any).continuumErrorCount = errorCount;
      
      this.forwardConsoleLog('promise-rejection', [{
        reason: event.reason,
        promise: 'Promise rejection'
      }]);
    });

    console.log('🚨 Console error capture initialized - all errors will forward to development portal');
  }

  private forwardConsoleLog(type: string, args: any[]): void {
    try {
      if (this.isConnected()) {
        // Capture EVERYTHING - full data structures, stack traces, source locations
        const stackTrace = new Error().stack;
        const sourceLocation = this.getSourceLocation(stackTrace);
        
        const consoleData = {
          type,
          timestamp: new Date().toISOString(),
          level: type,
          
          // Capture full argument data with deep inspection
          args: args.map(arg => this.inspectArgument(arg)),
          
          // Raw string representation (what user sees)
          message: args.map(arg => {
            if (typeof arg === 'string') return arg;
            if (typeof arg === 'number' || typeof arg === 'boolean') return String(arg);
            if (arg === null) return 'null';
            if (arg === undefined) return 'undefined';
            if (typeof arg === 'function') return arg.toString();
            try {
              return JSON.stringify(arg, null, 2);
            } catch {
              return String(arg);
            }
          }).join(' '),
          
          // Stack trace and source location
          stackTrace: stackTrace || 'No stack trace available',
          sourceLocation,
          
          // Browser context
          url: window.location.href,
          userAgent: navigator.userAgent,
          viewport: {
            width: window.innerWidth,
            height: window.innerHeight
          },
          
          // Error-specific data for errors
          ...(type === 'error' && args.length > 0 && args[0] instanceof Error ? {
            errorDetails: {
              name: args[0].name,
              message: args[0].message,
              stack: args[0].stack,
              cause: (args[0] as any).cause
            }
          } : {})
        };

        // Forward everything to development portal
        this.execute('console', {
          action: 'browser_console',
          message: `[${type.toUpperCase()}] ${consoleData.message}`,
          source: 'console-complete-capture',
          data: consoleData
        }).catch(() => {
          // Fail silently to avoid infinite loops
        });
      }
    } catch (error) {
      // Fail silently to avoid error loops
    }
  }

  private inspectArgument(arg: any): any {
    try {
      // Handle different types with full inspection
      if (arg === null) return { type: 'null', value: null };
      if (arg === undefined) return { type: 'undefined', value: undefined };
      
      const type = typeof arg;
      
      if (type === 'string' || type === 'number' || type === 'boolean') {
        return { type, value: arg };
      }
      
      if (type === 'function') {
        return { 
          type: 'function', 
          name: arg.name || 'anonymous',
          value: arg.toString(),
          length: arg.length
        };
      }
      
      if (arg instanceof Error) {
        return {
          type: 'Error',
          name: arg.name,
          message: arg.message,
          stack: arg.stack,
          cause: (arg as any).cause
        };
      }
      
      if (Array.isArray(arg)) {
        return {
          type: 'Array',
          length: arg.length,
          value: arg.map(item => this.inspectArgument(item)),
          preview: `Array(${arg.length})`
        };
      }
      
      if (arg instanceof Date) {
        return {
          type: 'Date',
          value: arg.toISOString(),
          timestamp: arg.getTime()
        };
      }
      
      if (arg instanceof Promise) {
        return {
          type: 'Promise',
          state: 'unknown',
          value: '[Promise object]'
        };
      }
      
      if (type === 'object') {
        // Deep object inspection
        const keys = Object.keys(arg);
        const result = {
          type: 'Object',
          constructor: arg.constructor?.name || 'Object',
          keys: keys,
          length: keys.length,
          value: {} as any,
          preview: `{${keys.slice(0, 3).join(', ')}${keys.length > 3 ? '...' : ''}}`
        };
        
        // Capture first level of properties (avoid infinite recursion)
        for (const key of keys.slice(0, 10)) { // Limit to 10 properties
          try {
            result.value[key] = this.limitDepthInspection(arg[key], 2);
          } catch {
            result.value[key] = '[Inspection failed]';
          }
        }
        
        return result;
      }
      
      return { type, value: String(arg) };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return { type: 'unknown', value: '[Inspection error]', error: errorMessage };
    }
  }
  
  private limitDepthInspection(value: any, maxDepth: number): any {
    if (maxDepth <= 0) return '[Max depth reached]';
    
    if (value === null || value === undefined) return value;
    if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') return value;
    if (typeof value === 'function') return '[Function]';
    if (Array.isArray(value)) return value.slice(0, 5).map(item => this.limitDepthInspection(item, maxDepth - 1));
    
    if (typeof value === 'object') {
      const result: any = {};
      const keys = Object.keys(value).slice(0, 5);
      for (const key of keys) {
        result[key] = this.limitDepthInspection(value[key], maxDepth - 1);
      }
      return result;
    }
    
    return String(value);
  }
  
  async discoverAndLoadWidgets(): Promise<void> {
    console.log('🔍 Loading core widgets...');
    
    // Load core widgets first using Promise.all for parallel loading
    const coreWidgets = [
      './components/Sidebar/SidebarWidget.js',
      './components/Chat/ChatWidget.js'
    ];
    
    const widgetPromises = coreWidgets.map(async (widgetPath) => {
      try {
        console.log(`📦 Loading core widget: ${widgetPath}`);
        await import(widgetPath);
        console.log(`✅ Successfully loaded: ${widgetPath}`);
        return { path: widgetPath, success: true };
      } catch (error) {
        console.warn(`⚠️ Failed to load core widget ${widgetPath}:`, error);
        return { path: widgetPath, success: false, error };
      }
    });
    
    // Wait for all core widgets to load (with individual error handling)
    const results = await Promise.allSettled(widgetPromises);
    const loadedCount = results.filter((r, i) => {
      if (r.status === 'fulfilled' && r.value.success) {
        return true;
      } else {
        const widgetPath = coreWidgets[i];
        console.warn(`⚠️ Widget ${widgetPath} failed:`, r.status === 'rejected' ? r.reason : r.value.error);
        return false;
      }
    }).length;
    
    console.log(`✅ Core widgets loaded: ${loadedCount}/${coreWidgets.length}`);
    
    // Register fallbacks ONLY if some widgets failed to load
    if (loadedCount < coreWidgets.length) {
      console.log(`🔧 Some widgets failed (${loadedCount}/${coreWidgets.length}) - registering fallbacks...`);
      try {
        await Promise.allSettled([
          import('./components/shared/WidgetFallbacks.js'),
          import('./components/shared/WidgetServerControls.js'),
          import('./components/shared/InteractivePersona.js')
        ]);
        console.log('✅ Widget fallbacks registered for failed widgets');
      } catch (error) {
        console.warn('⚠️ Failed to load widget fallbacks:', error);
      }
    } else {
      console.log('✅ All core widgets loaded successfully - no fallbacks needed');
    }
    
    // Try dynamic discovery as backup
    try {
      const widgetPaths = await this.discoverWidgetPaths();
      const discoveryPromises = widgetPaths.map(async (path) => {
        try {
          console.log(`📦 Loading discovered widget: ${path}`);
          await import(path);
          return { path, success: true };
        } catch (error) {
          console.warn(`⚠️ Failed to load discovered widget ${path}:`, error);
          return { path, success: false, error };
        }
      });
      
      const discoveryResults = await Promise.allSettled(discoveryPromises);
      const discoveredCount = discoveryResults.filter((r, i) => {
        if (r.status === 'fulfilled' && r.value.success) {
          return true;
        } else {
          const widgetPath = widgetPaths[i];
          console.warn(`⚠️ Discovery widget ${widgetPath} failed:`, r.status === 'rejected' ? r.reason : r.value.error);
          return false;
        }
      }).length;
      console.log(`✅ Discovery widgets loaded: ${discoveredCount}/${widgetPaths.length}`);
    } catch (error) {
      console.warn('⚠️ Dynamic widget discovery failed:', error);
    }
    
    console.log(`✅ Widget loading complete - ${loadedCount} widgets loaded`);
    
    // Widgets are already instantiated in the HTML - no need to create duplicates
    console.log('🎨 Widgets ready (instantiated via HTML)');
  }
  
  private async discoverWidgetPaths(): Promise<string[]> {
    try {
      // Ask the server to discover widgets via command (no hard-coded paths)
      if (this.isConnected()) {
        const response = await this.execute('discover_widgets');
        if (response && response.success && response.widgets) {
          return response.widgets;
        }
      }
    } catch (error) {
      console.warn('⚠️ Server widget discovery failed:', error);
    }
    
    // If server discovery fails, widgets simply won't load
    // No fallback hard-coding - maintains separation of concerns
    return [];
  }
  
  private getSourceLocation(stackTrace?: string): string {
    if (!stackTrace) return 'Unknown location';
    
    const lines = stackTrace.split('\n');
    // Find the first line that's not this file or console methods
    for (const line of lines) {
      if (line.includes('http') && !line.includes('continuum-browser') && !line.includes('console')) {
        return line.trim();
      }
    }
    
    return lines[2] || 'Unknown location';
  }

  private testConsoleCaptureSystem(): { allTestsPassed: boolean; summary: string; details: any } {
    const tests = {
      logOverride: false,
      errorOverride: false,
      stackTraceGeneration: false,
      sourceLocationDetection: false,
      dataInspection: false,
      forwardingCapability: false
    };

    try {
      // Test 1: Check if console.log has been overridden for capture
      const originalLogString = console.log.toString();
      tests.logOverride = !originalLogString.includes('[native code]') || 
                         originalLogString.includes('forwardConsoleLog');

      // Test 2: Check if console.error has been overridden 
      const originalErrorString = console.error.toString();
      tests.errorOverride = !originalErrorString.includes('[native code]') || 
                           originalErrorString.includes('forwardConsoleLog');

      // Test 3: Test stack trace generation
      try {
        const testError = new Error('Test stack trace');
        tests.stackTraceGeneration = !!testError.stack && testError.stack.length > 0;
      } catch {
        tests.stackTraceGeneration = false;
      }

      // Test 4: Test source location detection
      try {
        const mockStack = `Error: test
    at testFunction (http://localhost:9000/src/test.js:123:45)
    at main (http://localhost:9000/app.js:67:89)`;
        const location = this.getSourceLocation(mockStack);
        tests.sourceLocationDetection = location.includes('http://localhost:9000/src/test.js');
      } catch {
        tests.sourceLocationDetection = false;
      }

      // Test 5: Test data inspection capability
      try {
        const testData = { complex: { nested: true }, array: [1, 2, 3] };
        const inspected = this.inspectArgument(testData);
        tests.dataInspection = inspected && inspected.type === 'Object' && inspected.keys;
      } catch {
        tests.dataInspection = false;
      }

      // Test 6: Test forwarding capability (WebSocket + execute method)
      tests.forwardingCapability = this.isConnected() && typeof this.execute === 'function';

    } catch (error) {
      console.error('Console capture system test failed:', error);
    }

    const passedTests = Object.values(tests).filter(Boolean).length;
    const totalTests = Object.keys(tests).length;
    const allTestsPassed = passedTests === totalTests;

    return {
      allTestsPassed,
      summary: `${passedTests}/${totalTests} tests passed`,
      details: tests
    };
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
          
          // Send client initialization with version
          this.sendMessage({
            type: 'client_init',
            data: {
              userAgent: navigator.userAgent,
              url: window.location.href,
              timestamp: new Date().toISOString(),
              version: this.version
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

    // 5. CONSOLE CAPTURE SYSTEM VALIDATION
    const consoleErrors = (window as any).continuumErrorCount || 0;
    const consoleCaptureResults = this.testConsoleCaptureSystem();
    
    healthReport.components.push({
      component: 'console-capture-system',
      status: consoleCaptureResults.allTestsPassed ? 'healthy' : 'failed',
      lastCheck: Date.now(),
      details: `Console capture: ${consoleCaptureResults.summary}`,
      metrics: {
        errorCount: consoleErrors,
        consoleCaptureWorking: consoleCaptureResults.allTestsPassed,
        consoleTestResults: consoleCaptureResults.details
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
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.log('🏥 Could not forward health report to portal:', errorMessage);
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
          
          if (data.success) {
            resolve(data.data || { success: true });
          } else {
            reject(new Error(data.error || 'Command failed'));
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

      // Handle version mismatch - reload page
      if (message.type === 'version_mismatch') {
        const serverVersion = message.data?.serverVersion || 'unknown';
        console.log(`🔄 Version mismatch detected - Server: ${serverVersion}, Client: ${this.version} - Reloading...`);
        window.location.reload();
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
const continuum = new ContinuumBrowserAPI();
console.log(`🌐 Continuum Browser API v${continuum.version}: Creating global instance...`);

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
    
    // Report health back to server via simple HTTP (bypass WebSocket command timeout)
    try {
      const response = await fetch('/api/health', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          clientReport: healthReport,
          source: 'browser-auto-validation'
        })
      });
      const serverHealth = await response.json();
      console.log('🏥 Server health:', serverHealth.status);
    } catch (error) {
      console.log('🏥 Could not forward health report to server:', error instanceof Error ? error.message : String(error));
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

// Dynamic widget discovery and loading
continuum.discoverAndLoadWidgets().then(() => {
  console.log('🎨 Widget system ready - widgets dynamically discovered');
}).catch((error) => {
  console.error('❌ Widget discovery failed:', error);
});

export default continuum;