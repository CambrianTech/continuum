/**
 * Emergency JTAG - Universal Base Implementation
 * 
 * STANDALONE DEBUGGING SYSTEM - NO CONTINUUM DEPENDENCIES
 * Automatically detects browser/server context and provides unified API.
 * Import once, works everywhere.
 */

import { JTAGLogEntry, JTAGConfig, JTAGStats, JTAGExecOptions, JTAGExecResult, JTAGUUIDInfo, JTAGScreenshotOptions, JTAGScreenshotResult, JTAGStatusEvent, JTAGStatusEventListener, JTAG_STATUS, JTAGStatus, JTAG_TRANSPORT, JTAGTransportType, ContinuumConnectionParams, ContinuumConnection, DEFAULT_CONNECTION_PARAMS, JTAGMessageFactory, JTAG_MESSAGE_TYPES, JTAG_CONTEXTS, JTAGUniversalMessage, JTAGLogPayload } from './JTAGTypes';
import { jtagConfig } from './config';
import { jtagRouter } from './JTAGRouter';
import { JTAGWebSocketServer, JTAGWebSocketClient } from './JTAGWebSocket';

export class JTAGBase {
  private static config: JTAGConfig;
  private static initialized = false;
  private static serverStarted = false;  // Prevent multiple server instances
  private static logBuffer: JTAGLogEntry[] = [];
  private static isServer: boolean;
  private static isClient: boolean;
  private static instanceUUID: string;
  private static sessionId: string;
  private static webSocketServer: JTAGWebSocketServer | null = null;
  private static webSocketClient: JTAGWebSocketClient | null = null;

  /**
   * Status Event System
   */
  private static statusListeners: JTAGStatusEventListener[] = [];
  private static currentStatus: JTAGStatus = JTAG_STATUS.DISCONNECTED;

  /**
   * Transport Message Queueing System - Hold messages until transport route is ready
   */
  private static transportMessageQueue: JTAGUniversalMessage[] = [];
  private static transportReady = false;
  private static routeAvailable = false;

  /**
   * HTML2Canvas Static Properties
   */
  private static html2canvasLoaded = false;
  private static html2canvasPromise: Promise<any> | null = null;

  /**
   * Generate UUID for JTAG instance
   */
  private static generateUUID(): string {
    return 'jtag_' + Date.now().toString(36) + '_' + Math.random().toString(36).substr(2, 9);
  }

  /**
   * Status Event Management
   */
  static addStatusListener(listener: JTAGStatusEventListener): void {
    this.statusListeners.push(listener);
  }

  static removeStatusListener(listener: JTAGStatusEventListener): void {
    const index = this.statusListeners.indexOf(listener);
    if (index >= 0) {
      this.statusListeners.splice(index, 1);
    }
  }

  private static emitStatusEvent(status: JTAGStatus, transportOverride?: JTAGTransportType, details?: any): void {
    this.currentStatus = status;
    
    // Get transport info from active transport or use override
    const transportType = transportOverride || JTAG_TRANSPORT.WEBSOCKET;
    const connectionState = this.getActiveTransportState();
    
    const event: JTAGStatusEvent = {
      status,
      uuid: this.instanceUUID,
      sessionId: this.sessionId,
      timestamp: Date.now(),
      context: this.isServer ? 'server' : 'browser',
      transport: {
        type: transportType,
        state: connectionState,
        details
      }
    };

    // Emit to internal listeners
    this.statusListeners.forEach(listener => {
      try {
        listener(event);
      } catch (error) {
        console.error('JTAG Status Event Listener Error:', error);
      }
    });

    // Emit browser DOM event if in browser context
    if (this.isClient && typeof window !== 'undefined' && typeof CustomEvent !== 'undefined') {
      const domEvent = new CustomEvent('jtag:status', { detail: event });
      window.dispatchEvent(domEvent);
      
      // Also emit specific status events
      const specificEvent = new CustomEvent(`jtag:${status}`, { detail: event });
      window.dispatchEvent(specificEvent);
    }

    console.log(`🚨 JTAG Status: ${status.toUpperCase()} [${transportType}]`, event);
  }

  private static getActiveTransportState() {
    // Try to get state from active transport
    if (this.webSocketServer) {
      return {
        connected: true,
        endpoint: `ws://localhost:${this.config.jtagPort}`,
        protocol: 'websocket',
        lastActivity: Date.now()
      };
    }
    
    if (this.webSocketClient?.isConnected?.()) {
      return {
        connected: true,
        endpoint: `ws://localhost:${this.config.jtagPort}`,
        protocol: 'websocket',
        lastActivity: Date.now()
      };
    }
    
    return {
      connected: false,
      lastActivity: Date.now()
    };
  }

  static getStatus(): JTAGStatus {
    return this.currentStatus;
  }

  /**
   * Auto-detect context and initialize JTAG
   */
  static initialize(overrides?: Partial<JTAGConfig>): void {
    console.log('🔧 JTAG Base: initialize() called');
    
    // Set up transport readiness monitoring
    this.setupTransportMonitoring();
    
    // Auto-detect context
    this.isServer = typeof require !== 'undefined' && typeof window === 'undefined';
    this.isClient = typeof window !== 'undefined';
    
    const context = this.isServer ? 'server' : 'browser';
    console.log('🔧 JTAG Base: Context detected -', context, 'isServer:', this.isServer, 'isClient:', this.isClient);
    
    // Generate unique IDs
    this.instanceUUID = this.generateUUID();
    this.sessionId = 'session_' + Date.now().toString(36);
    console.log('🔧 JTAG Base: Generated UUID:', this.instanceUUID, 'Session:', this.sessionId);
    
    this.config = {
      ...jtagConfig,
      context,
      logDirectory: this.isServer ? jtagConfig.logDirectory : undefined,
      ...overrides
    } as JTAGConfig;

    console.log('🔧 JTAG Base: Config created:', this.config);
    this.initialized = true;

    // Emit connecting status when starting initialization
    this.emitStatusEvent(JTAG_STATUS.CONNECTING, JTAG_TRANSPORT.WEBSOCKET, { 
      reason: 'initialization_started' 
    });

    // Register appropriate transports with the router
    this.registerTransports();

    if (this.isServer) {
      console.log('🔧 JTAG Base: Starting server logging...');
      // Start WebSocket server async to avoid blocking
      this.startServerLogging().catch(error => {
        console.error('🚨 JTAG Base: Server logging failed:', error.message);
      });
    }

    if (this.isClient && this.config.enableRemoteLogging) {
      console.log('🔧 JTAG Base: Client context with remote logging enabled - starting WebSocket initialization');
      // Initialize async WebSocket connection (don't wait to avoid blocking)
      this.initializeRemoteLogging().catch(error => {
        console.warn('🌐 JTAG Browser WebSocket initialization failed:', error.message);
      });
    } else if (this.isClient) {
      console.log('🔧 JTAG Base: Client context but remote logging disabled');
    }

    this.log('JTAG', `JTAG initialized for ${context} context`, {
      uuid: this.instanceUUID,
      sessionId: this.sessionId
    });
    console.log('🔧 JTAG Base: Initialization complete');
  }

  /**
   * Register appropriate transports with the router based on context
   */
  private static registerTransports(): void {
    if (this.isServer) {
      // Server context - register server transport for file logging
      try {
        const { JTAGServerTransport } = require('../server/JTAGServerTransport');
        const serverTransport = new JTAGServerTransport(this.config.logDirectory);
        jtagRouter.registerTransport(serverTransport);
        console.log('🔌 JTAG Base: Registered server transport for file logging');
      } catch (error) {
        console.warn('⚠️ JTAG Base: Failed to register server transport:', error);
      }
    }

    if (this.isClient && this.config.enableRemoteLogging) {
      // Client context - register client transport for WebSocket communication
      try {
        const { JTAGClientTransport } = require('../client/JTAGClientTransport');
        const clientTransport = new JTAGClientTransport(this.config.jtagPort);
        jtagRouter.registerTransport(clientTransport);
        console.log('🔌 JTAG Base: Registered client transport for remote logging');
      } catch (error) {
        console.warn('⚠️ JTAG Base: Failed to register client transport:', error);
      }
    }
  }

  /**
   * Universal logging - same call everywhere, routes through typed router
   */
  static log(component: string, message: string, data?: any): void {
    this.ensureInitialized();
    
    // Create universal JTAG message using factory
    const logMessage = JTAGMessageFactory.createRequest(
      JTAG_MESSAGE_TYPES.LOG,
      this.config.context === 'browser' ? JTAG_CONTEXTS.BROWSER : JTAG_CONTEXTS.SERVER,
      {
        level: 'log' as const,
        message,
        component,
        data
      } as JTAGLogPayload
    );

    // Route through universal router
    this.routeMessage(logMessage);
  }

  /**
   * Universal message routing - shared by browser and server
   * Queues messages until transport route is available
   */
  private static async routeMessage(message: JTAGUniversalMessage): Promise<void> {
    // Check if route is available for this message type
    if (!this.isRouteAvailable(message)) {
      // Queue the message until route becomes available
      this.transportMessageQueue.push(message);
      
      if (this.config.enableConsoleOutput) {
        const emoji = this.getLogEmoji(message);
        this.consoleOriginals.log?.(`🔄 JTAG ${emoji} [${(message.payload as any).component}]: ${(message.payload as any).message} (queued - route not ready)`);
      }
      return;
    }
    
    try {
      // Route through the universal JTAG router
      const results = await jtagRouter.routeMessage(message);
      
      // Process successful routes for console output
      results.forEach(result => {
        if (result.success && this.config.enableConsoleOutput) {
          const emoji = this.getLogEmoji(message);
          this.consoleOriginals.log?.(`🚨 JTAG ${emoji} [${(message.payload as any).component}]: ${(message.payload as any).message}${(message.payload as any).data ? ' ' + JSON.stringify((message.payload as any).data) : ''}`);
        }
      });
      
    } catch (error) {
      console.error('🚨 JTAG Router: Message routing failed:', error);
    }
  }

  /**
   * Check if route is available for message type
   */
  private static isRouteAvailable(message: JTAGUniversalMessage): boolean {
    // Server-side: Always route immediately (logs to files, optionally broadcasts)
    if (this.isServer) {
      return true;
    }
    
    // Client-side: Wait for transport to be ready for cross-transport messaging
    return this.transportReady && this.routeAvailable;
  }

  /**
   * Set up transport readiness monitoring
   */
  private static setupTransportMonitoring(): void {
    // Listen for READY status events to flush queue
    this.addStatusListener((event: JTAGStatusEvent) => {
      if (event.status === JTAG_STATUS.READY) {
        this.setTransportReady().catch(error => {
          console.error('🚨 JTAG: Transport ready queue flush failed:', error);
        });
      }
    });
  }

  /**
   * Mark transport as ready and flush queued messages (client-side only)
   */
  private static async setTransportReady(): Promise<void> {
    this.transportReady = true;
    this.routeAvailable = true;
    
    // Only flush queue for client-side (server logs immediately)
    if (this.isClient && this.transportMessageQueue.length > 0) {
      this.consoleOriginals.log?.(`🚀 JTAG: Flushing ${this.transportMessageQueue.length} queued messages...`);
      
      // Flush all queued messages
      const queuedMessages = [...this.transportMessageQueue];
      this.transportMessageQueue = [];
      
      for (const message of queuedMessages) {
        await this.routeMessage(message);
      }
      
      this.consoleOriginals.log?.(`✅ JTAG: Flushed queued messages`);
    }
  }

  /**
   * Get emoji for log message type
   */
  private static getLogEmoji(message: JTAGUniversalMessage): string {
    const level = (message.payload as any).level;
    switch (level) {
      case 'critical': return '🔥';
      case 'trace': return '🔍';
      case 'probe': return '📊';
      case 'test': return '🧪';
      case 'error': return '❌';
      case 'warn': return '⚠️';
      default: return '📝';
    }
  }

  /**
   * Critical event logging
   */
  static critical(component: string, event: string, data?: any): void {
    this.ensureInitialized();

    const logMessage = JTAGMessageFactory.createRequest(
      JTAG_MESSAGE_TYPES.LOG,
      this.config.context === 'browser' ? JTAG_CONTEXTS.BROWSER : JTAG_CONTEXTS.SERVER,
      {
        level: 'critical' as const,
        message: event,
        component,
        data
      } as JTAGLogPayload
    );

    this.routeMessage(logMessage);
  }

  static warn(component: string, message: string, data?: any): void {
    this.ensureInitialized();

    const logMessage = JTAGMessageFactory.createRequest(
      JTAG_MESSAGE_TYPES.LOG,
      this.config.context === 'browser' ? JTAG_CONTEXTS.BROWSER : JTAG_CONTEXTS.SERVER,
      {
        level: 'warn' as const,
        message,
        component,
        data
      } as JTAGLogPayload
    );

    this.routeMessage(logMessage);
  }

  /**
   * Error logging - maps to console.error
   */
  static error(component: string, message: string, data?: any): void {
    this.ensureInitialized();

    const logMessage = JTAGMessageFactory.createRequest(
      JTAG_MESSAGE_TYPES.LOG,
      this.config.context === 'browser' ? JTAG_CONTEXTS.BROWSER : JTAG_CONTEXTS.SERVER,
      {
        level: 'error' as const,
        message,
        component,
        data
      } as JTAGLogPayload
    );

    this.routeMessage(logMessage);
  }

  /**
   * Function execution tracing
   */
  static trace(component: string, functionName: string, phase: 'ENTER' | 'EXIT', data?: any): void {
    this.ensureInitialized();

    const logMessage = JTAGMessageFactory.createRequest(
      JTAG_MESSAGE_TYPES.LOG,
      this.config.context === 'browser' ? JTAG_CONTEXTS.BROWSER : JTAG_CONTEXTS.SERVER,
      {
        level: 'trace' as const,
        message: `TRACE: ${functionName} ${phase}`,
        component,
        data
      } as JTAGLogPayload
    );

    this.routeMessage(logMessage);
  }

  /**
   * System state probes
   */
  static probe(component: string, probeName: string, state: any): void {
    this.ensureInitialized();

    const logMessage = JTAGMessageFactory.createRequest(
      JTAG_MESSAGE_TYPES.LOG,
      this.config.context === 'browser' ? JTAG_CONTEXTS.BROWSER : JTAG_CONTEXTS.SERVER,
      {
        level: 'probe' as const,
        message: `PROBE: ${probeName}`,
        component,
        data: state
      } as JTAGLogPayload
    );

    this.routeMessage(logMessage);
  }

  /**
   * Test-specific logging for verification and debugging
   */
  static test(component: string, testName: string, data?: any): void {
    this.ensureInitialized();

    const logMessage = JTAGMessageFactory.createRequest(
      JTAG_MESSAGE_TYPES.LOG,
      this.config.context === 'browser' ? JTAG_CONTEXTS.BROWSER : JTAG_CONTEXTS.SERVER,
      {
        level: 'test' as const,
        message: `TEST: ${testName}`,
        component,
        data
      } as JTAGLogPayload
    );

    this.routeMessage(logMessage);
  }

  /**
   * Execute JavaScript code with result capture
   */
  static async exec(code: string, options: JTAGExecOptions = {}): Promise<JTAGExecResult> {
    this.ensureInitialized();
    
    const startTime = Date.now();
    const execUUID = options.uuid || this.generateUUID();
    
    const baseResult: JTAGExecResult = {
      success: false,
      context: this.config.context,
      timestamp: new Date().toISOString(),
      executionTime: 0,
      uuid: execUUID
    };

    try {
      this.trace('JTAG_EXEC', 'codeExecution', 'ENTER', { 
        code: code.substring(0, 100),
        uuid: execUUID,
        options 
      });

      let result: any;
      
      if (this.isServer) {
        result = await this.executeServerCode(code, options);
      } else if (this.isClient) {
        result = await this.executeBrowserCode(code, options);
      } else {
        throw new Error('Unknown execution context');
      }

      const executionTime = Date.now() - startTime;
      
      this.trace('JTAG_EXEC', 'codeExecution', 'EXIT', { 
        result: typeof result,
        executionTime,
        uuid: execUUID
      });

      return {
        ...baseResult,
        success: true,
        result: options.returnValue !== false ? result : undefined,
        executionTime
      };

    } catch (error: any) {
      const executionTime = Date.now() - startTime;
      
      this.critical('JTAG_EXEC', 'Code execution failed', { 
        error: error.message,
        code: code.substring(0, 50),
        uuid: execUUID
      });

      return {
        ...baseResult,
        error: error.message,
        executionTime
      };
    }
  }

  /**
   * Get JTAG instance UUID and metadata
   */
  static getUUID(): JTAGUUIDInfo {
    this.ensureInitialized();
    
    const result: JTAGUUIDInfo = {
      uuid: this.instanceUUID,
      context: this.config.context,
      timestamp: new Date().toISOString(),
      sessionId: this.sessionId,
      metadata: {
        initialized: this.initialized,
        serverStarted: this.serverStarted,
        logBufferSize: this.logBuffer.length,
        config: this.config
      }
    };
    
    // Conditionally add processId to avoid undefined assignment with exactOptionalPropertyTypes
    if (this.isServer && process?.pid) {
      result.processId = process.pid;
    }
    
    return result;
  }

  /**
   * Console interception storage
   */
  private static consoleOriginals: {
    log?: Function;
    error?: Function;
    warn?: Function;
    info?: Function;
  } = {};
  private static consoleAttached: boolean = false;

  /**
   * Attach JTAG to console methods (works on both client and server)
   * 
   * CRITICAL FIX: Proper console.log → originalConsole → jtag.log routing
   * 
   * Example flow for client-side console.log("this is my message"):
   * 1. console.log("this is my message") 
   * 2. calls originalConsole.log("this is my message") first (normal console output)
   * 3. then calls jtag.log to route via WebSocket or queue
   * 4. server receives JtagMessage, calls logger with proper platform.level files
   */
  static attach(consoleObject: Console): void {
    this.ensureInitialized();
    
    if (this.consoleAttached) {
      console.warn('JTAG_ATTACH: Console already attached, detaching first');
      this.detach(consoleObject);
    }
    
    // Store original console methods BEFORE attaching JTAG
    this.consoleOriginals.log = consoleObject.log.bind(consoleObject);
    this.consoleOriginals.error = consoleObject.error.bind(consoleObject);
    this.consoleOriginals.warn = consoleObject.warn.bind(consoleObject);
    this.consoleOriginals.info = consoleObject.info.bind(consoleObject);
    
    const self = this;
    
    // Attach console.log - FIRST call original, THEN route to JTAG
    consoleObject.log = function(...args: any[]) {
      // FIRST: Call original console.log for normal output
      self.consoleOriginals.log?.(...args);
      
      // THEN: Route to JTAG system (this calls jtag.log)
      self.log('CONSOLE_LOG', args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' '));
    };
    
    // Attach console.error - FIRST call original, THEN route to JTAG  
    consoleObject.error = function(...args: any[]) {
      // FIRST: Call original console.error for normal output
      self.consoleOriginals.error?.(...args);
      
      // THEN: Route to JTAG system (this calls jtag.error for console errors)
      self.error('CONSOLE_ERROR', args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' '));
    };
    
    // Attach console.warn - FIRST call original, THEN route to JTAG
    consoleObject.warn = function(...args: any[]) {
      // FIRST: Call original console.warn for normal output
      self.consoleOriginals.warn?.(...args);
      
      // THEN: Route to JTAG system (this calls jtag.warn)
      self.warn('CONSOLE_WARN', args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' '));
    };
    
    // Attach console.info - FIRST call original, THEN route to JTAG
    consoleObject.info = function(...args: any[]) {
      // FIRST: Call original console.info for normal output
      self.consoleOriginals.info?.(...args);
      
      // THEN: Route to JTAG system (this calls jtag.log for info)
      self.log('CONSOLE_INFO', args.map(a => typeof a === 'object' ? JSON.stringify(a) : String(a)).join(' '));
    };
    
    this.consoleAttached = true;
    
    // Use original console for this message to avoid recursion
    this.consoleOriginals.log?.('🔧 JTAG Console interception attached for context:', this.config.context);
  }

  /**
   * Detach JTAG from console methods (restores original behavior)
   */
  static detach(consoleObject: Console): void {
    if (!this.consoleAttached) {
      return;
    }
    
    // Restore original console methods
    if (this.consoleOriginals.log) consoleObject.log = this.consoleOriginals.log as any;
    if (this.consoleOriginals.error) consoleObject.error = this.consoleOriginals.error as any;
    if (this.consoleOriginals.warn) consoleObject.warn = this.consoleOriginals.warn as any;
    if (this.consoleOriginals.info) consoleObject.info = this.consoleOriginals.info as any;
    
    // Clear stored references
    this.consoleOriginals = {};
    this.consoleAttached = false;
    
    this.log('JTAG_DETACH', 'Console interception detached', {
      context: this.config.context
    });
  }

  /**
   * Execute code in server context
   */
  private static async executeServerCode(code: string, options: JTAGExecOptions): Promise<any> {
    try {
      // Create safe execution context with JTAG available
      const contextGlobals = {
        jtag: {
          log: this.log.bind(this),
          critical: this.critical.bind(this),
          trace: this.trace.bind(this),
          probe: this.probe.bind(this),
          screenshot: this.screenshot.bind(this),
          getUUID: this.getUUID.bind(this),
          exec: this.exec.bind(this)
        },
        console,
        require: this.isServer ? require : undefined,
        process: this.isServer ? process : undefined,
        Buffer: this.isServer ? Buffer : undefined
      };

      // Create function with available context
      const func = new Function(...Object.keys(contextGlobals), `return (${code})`);
      const result = func(...Object.values(contextGlobals));
      
      // Handle promises
      if (result && typeof result.then === 'function') {
        return await Promise.race([
          result,
          new Promise((_, reject) => setTimeout(reject, options.timeout || 5000, new Error('Execution timeout')))
        ]);
      }
      
      return result;
    } catch (error) {
      throw error;
    }
  }

  /**
   * Execute code in browser context  
   */
  private static async executeBrowserCode(code: string, options: JTAGExecOptions): Promise<any> {
    try {
      // For browser context, send to server for execution if needed
      if (options.context === 'server' && typeof fetch !== 'undefined') {
        const response = await fetch(`http://localhost:${this.config.jtagPort}/exec`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ code, options })
        });
        
        if (response.ok) {
          const result = await response.json();
          return result.result;
        } else {
          throw new Error(`Server execution failed: ${response.statusText}`);
        }
      }

      // Direct browser execution
      const contextGlobals = {
        jtag: {
          log: this.log.bind(this),
          critical: this.critical.bind(this),
          trace: this.trace.bind(this),
          probe: this.probe.bind(this),
          screenshot: this.screenshot.bind(this),
          getUUID: this.getUUID.bind(this),
          exec: this.exec.bind(this)
        },
        console,
        window: typeof window !== 'undefined' ? window : undefined,
        document: typeof document !== 'undefined' ? document : undefined
      };

      const func = new Function(...Object.keys(contextGlobals), `return (${code})`);
      const result = func(...Object.values(contextGlobals));
      
      if (result && typeof result.then === 'function') {
        return await Promise.race([
          result,
          new Promise((_, reject) => setTimeout(reject, options.timeout || 5000, new Error('Execution timeout')))
        ]);
      }
      
      return result;
    } catch (error) {
      throw error;
    }
  }

  /**
   * Screenshot capture with async/await support
   */
  static async screenshot(filename?: string, options?: JTAGScreenshotOptions): Promise<JTAGScreenshotResult> {
    this.ensureInitialized();
    
    const screenshotName = filename || `jtag-debug-${Date.now()}`;
    const timestamp = new Date().toISOString();
    
    this.log('JTAG', `Screenshot requested via JTAG`, { 
      context: this.config.context,
      filename: screenshotName,
      options 
    });
    
    if (this.isClient) {
      return await this.captureBrowserScreenshot(screenshotName, options || {}, timestamp);
    } else {
      return await this.captureServerScreenshot(screenshotName, options || {}, timestamp);
    }
  }

  /**
   * Connect to JTAG system with health verification
   * Works on both client and server contexts
   */
  static async connect(params?: ContinuumConnectionParams): Promise<ContinuumConnection> {
    this.ensureInitialized();
    
    // Merge with defaults
    const config = { ...DEFAULT_CONNECTION_PARAMS, ...params };
    
    this.log('JTAG_CONNECT', 'Connection requested', { params: config });
    
    const startTime = Date.now();
    let latency = 0;
    let transportType: 'websocket' | 'rest' | 'mcp' | 'polling' | 'sse' = 'websocket';
    let endpoint = '';
    let connectionState: 'connected' | 'connecting' | 'disconnected' | 'error' = 'connecting';
    
    try {
      // Start connection attempt
      this.emitStatusEvent(JTAG_STATUS.CONNECTING, JTAG_TRANSPORT.WEBSOCKET, {
        reason: 'connect_method_called',
        params: config
      });
      
      if (config.transport === 'auto' || config.transport === 'websocket') {
        // Try WebSocket connection
        if (this.isClient) {
          await this.getWebSocketClient();
          endpoint = `ws://localhost:${this.config.jtagPort || 9001}`;
          
          // Perform health check if requested
          if (config.healthCheck) {
            const healthStart = Date.now();
            // Send ping message to verify connection health
            const healthResult = await this.sendTransportMessage('log', {
              component: 'HEALTH_CHECK',
              message: 'Connection health verification',
              type: 'test',
              timestamp: new Date().toISOString()
            });
            
            latency = Date.now() - healthStart;
            
            if (healthResult.success) {
              connectionState = 'connected';
              transportType = 'websocket';
              this.emitStatusEvent(JTAG_STATUS.READY, JTAG_TRANSPORT.WEBSOCKET, {
                reason: 'health_check_passed',
                latency,
                endpoint
              });
            } else {
              connectionState = 'error';
            }
          } else {
            // No health check - assume connected if WebSocket client exists
            connectionState = 'connected';
            latency = Date.now() - startTime;
            transportType = 'websocket';
          }
        } else {
          // Server-side - check if server is running
          endpoint = `ws://localhost:${this.config.jtagPort || 9001}`;
          
          if (this.webSocketServer) {
            connectionState = 'connected';
            transportType = 'websocket';
            latency = Date.now() - startTime;
            
            this.emitStatusEvent(JTAG_STATUS.READY, JTAG_TRANSPORT.WEBSOCKET, {
              reason: 'server_side_connection_verified',
              endpoint
            });
          } else {
            // Start server if not running
            await this.startServerLogging();
            connectionState = 'connected';
            transportType = 'websocket';
            latency = Date.now() - startTime;
          }
        }
      }
      
      // Build connection response
      const connection: ContinuumConnection = {
        healthy: connectionState === 'connected',
        transport: {
          type: transportType,
          state: connectionState,
          endpoint,
          latency
        },
        session: {
          id: this.sessionId,
          uuid: this.instanceUUID,
          uptime: Date.now() - startTime
        }
      };
      
      this.log('JTAG_CONNECT', 'Connection established', {
        healthy: connection.healthy,
        transport: connection.transport.type,
        latency: connection.transport.latency
      });
      
      return connection;
      
    } catch (error: any) {
      connectionState = 'error';
      
      this.emitStatusEvent(JTAG_STATUS.ERROR, JTAG_TRANSPORT.WEBSOCKET, {
        reason: 'connection_failed',
        error: error.message
      });
      
      this.error('JTAG_CONNECT', 'Connection failed', { error: error.message });
      
      // Return failed connection info
      return {
        healthy: false,
        transport: {
          type: transportType,
          state: 'error',
          endpoint: endpoint || 'unknown',
          latency: Date.now() - startTime
        },
        session: {
          id: this.sessionId,
          uuid: this.instanceUUID,
          uptime: 0
        }
      };
    }
  }

  /**
   * Dynamically load html2canvas library (browser-side screenshots)
   */
  private static async loadHtml2Canvas(): Promise<any> {
    if (typeof window === 'undefined') {
      throw new Error('html2canvas is only available in browser context');
    }

    // Return cached instance if already loaded
    if ((window as any).html2canvas && this.html2canvasLoaded) {
      this.log('JTAG', 'html2canvas already loaded (cached)');
      return (window as any).html2canvas;
    }

    // Return existing promise if already loading (executeOnce pattern)
    if (this.html2canvasPromise) {
      this.log('JTAG', 'html2canvas loading in progress (waiting for existing promise)');
      return this.html2canvasPromise;
    }

    // Create new loading promise (only executed once)
    this.log('JTAG', 'Starting html2canvas dynamic load...');
    this.html2canvasPromise = new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
      
      script.onload = () => {
        this.log('JTAG', '✅ html2canvas loaded successfully');
        this.html2canvasLoaded = true;
        const html2canvas = (window as any).html2canvas;
        if (html2canvas) {
          resolve(html2canvas);
        } else {
          this.critical('JTAG_HTML2CANVAS', 'html2canvas not available after script load');
          reject(new Error('html2canvas not available after load'));
        }
      };
      
      script.onerror = (error) => {
        this.critical('JTAG_HTML2CANVAS', 'Failed to load html2canvas from CDN', { error });
        reject(new Error('Failed to load html2canvas library'));
      };
      
      document.head.appendChild(script);
      this.log('JTAG', 'html2canvas script injected, waiting for load...');
    });

    return this.html2canvasPromise;
  }

  /**
   * Get or create WebSocket client (browser-side)
   */
  private static async getWebSocketClient(): Promise<JTAGWebSocketClient> {
    if (typeof window === 'undefined') {
      throw new Error('WebSocket client only available in browser context');
    }

    if (!this.webSocketClient) {
      this.webSocketClient = new JTAGWebSocketClient(this.config.jtagPort || 9001);
    }

    await this.webSocketClient.connect();
    return this.webSocketClient;
  }

  /**
   * Send message via WebSocket (browser-side)
   */
  private static async sendTransportMessage(type: 'log' | 'screenshot' | 'exec', payload: any): Promise<any> {
    // Create JTAG universal message using factory
    const source = typeof window === 'undefined' ? JTAG_CONTEXTS.SERVER : JTAG_CONTEXTS.BROWSER;
    const messageType = type === 'log' ? JTAG_MESSAGE_TYPES.LOG : 
                       type === 'screenshot' ? JTAG_MESSAGE_TYPES.SCREENSHOT :
                       JTAG_MESSAGE_TYPES.EXEC;
    
    const message = JTAGMessageFactory.createRequest(messageType, source, payload);

    if (typeof window === 'undefined') {
      // Server-side - route through JTAG router
      return await jtagRouter.routeMessage(message);
    } else {
      // Browser-side - send via WebSocket to JTAG router
      const client = await this.getWebSocketClient();
      
      switch (type) {
        case 'log':
          return await client.sendLog(payload);
        case 'screenshot':
          return await client.sendScreenshot(payload);
        case 'exec':
          return await client.sendExec(payload.code, payload.options);
        default:
          throw new Error(`Unknown transport message type: ${type}`);
      }
    }
  }

  /**
   * Find target element with fallback logic
   */
  private static findTargetElement(selector: string): Element {
    let element: Element | null = null;
    
    // Try querySelector first
    try {
      element = document.querySelector(selector);
    } catch (error: any) {
      this.warn('JTAG_SCREENSHOT', `Invalid selector: ${selector}`, { error: error.message });
    }
    
    // Fallback to body
    if (!element) {
      element = document.body;
    }
    
    return element;
  }

  /**
   * Browser-side screenshot using html2canvas
   */
  private static async captureBrowserScreenshot(
    filename: string, 
    options: JTAGScreenshotOptions, 
    timestamp: string
  ): Promise<JTAGScreenshotResult> {
    
    const baseResult: JTAGScreenshotResult = {
      success: false,
      filepath: `${jtagConfig.screenshotDirectory}/${filename}.png`,
      filename,
      context: 'browser',
      timestamp,
      options
    };

    try {
      // Load html2canvas
      const html2canvas = await this.loadHtml2Canvas();
      
      // Find target element
      const targetElement = this.findTargetElement(options.selector || 'body');
      
      // Apply delay if specified
      if (options.delay && options.delay > 0) {
        await new Promise(resolve => setTimeout(resolve, options.delay));
      }
      
      // Capture screenshot
      const canvas = await html2canvas(targetElement, {
        width: options.width,
        height: options.height,
        scale: options.quality || 1,
        useCORS: true,
        allowTaint: false,
        backgroundColor: null
      });
      
      // Convert to data URL
      const format = options.format || 'png';
      const quality = format === 'jpeg' ? (options.quality || 0.9) : undefined;
      const dataUrl = canvas.toDataURL(`image/${format}`, quality);
      
      // Calculate metadata
      const metadata = {
        width: canvas.width,
        height: canvas.height,
        size: dataUrl.length,
        selector: options.selector || 'body'
      };
      
      // Send to server via WebSocket
      try {
        const screenshotPayload = {
          filename,
          dataUrl,
          format,
          metadata,
          timestamp
        };
        
        await this.sendTransportMessage('screenshot', screenshotPayload);
        
        this.log('JTAG', `Screenshot sent to server: ${filename}.${format}`, metadata);
        
      } catch (saveError: any) {
        this.warn('JTAG_SCREENSHOT', 'Failed to send screenshot to server', { error: saveError.message });
        throw saveError;
      }
      
      return {
        ...baseResult,
        success: true,
        metadata,
        filepath: `${jtagConfig.screenshotDirectory}/${filename}.${format}`
      };
      
    } catch (error: any) {
      this.critical('JTAG_SCREENSHOT', 'Browser screenshot failed', { error: error.message, filename });
      return {
        ...baseResult,
        error: error.message
      };
    }
  }

  /**
   * Server-side screenshot placeholder
   */
  private static async captureServerScreenshot(
    filename: string, 
    options: JTAGScreenshotOptions, 
    timestamp: string
  ): Promise<JTAGScreenshotResult> {
    
    const filepath = `${jtagConfig.screenshotDirectory}/${filename}.txt`;
    
    const baseResult: JTAGScreenshotResult = {
      success: false,
      filepath,
      filename,
      context: 'server',
      timestamp,
      options
    };

    try {
      const fs = require('fs');
      if (!fs.existsSync(jtagConfig.screenshotDirectory)) {
        fs.mkdirSync(jtagConfig.screenshotDirectory, { recursive: true });
      }
      
      const placeholderContent = `JTAG Server Screenshot Placeholder
Filename: ${filename}
Context: server  
Timestamp: ${timestamp}
Options: ${JSON.stringify(options, null, 2)}

Note: Server screenshots require browser automation or headless browser setup.
For now, JTAG creates this placeholder to verify the screenshot system works.
To enable real server screenshots, integrate Puppeteer or similar headless browser.`;
      
      fs.writeFileSync(filepath, placeholderContent);
      
      const metadata = {
        width: options.width || 1024,
        height: options.height || 768,
        size: placeholderContent.length,
        selector: options.selector || 'body'
      };
      
      this.log('JTAG', `Server screenshot placeholder created: ${filepath}`, metadata);
      
      return {
        ...baseResult,
        success: true,
        metadata
      };
      
    } catch (error: any) {
      this.critical('JTAG_SCREENSHOT', 'Server screenshot failed', { error: error.message, filename });
      return {
        ...baseResult,
        error: error.message
      };
    }
  }

  /**
   * Handle screenshot payload from WebSocket
   */
  private static handleScreenshotPayload(payload: any): JTAGScreenshotResult {
    try {
      const { filename, dataUrl, format, metadata, timestamp } = payload;
      
      // Extract base64 data from data URL
      const base64Data = dataUrl.split(',')[1];
      const buffer = Buffer.from(base64Data, 'base64');
      
      // Ensure screenshot directory exists
      const fs = require('fs');
      if (!fs.existsSync(jtagConfig.screenshotDirectory)) {
        fs.mkdirSync(jtagConfig.screenshotDirectory, { recursive: true });
      }
      
      // Save image file
      const extension = format === 'jpeg' ? 'jpg' : format;
      const filepath = `${jtagConfig.screenshotDirectory}/${filename}.${extension}`;
      fs.writeFileSync(filepath, buffer);
      
      const result: JTAGScreenshotResult = {
        success: true,
        filepath,
        filename: `${filename}.${extension}`,
        context: 'browser',
        timestamp,
        metadata: {
          ...metadata,
          size: buffer.length
        }
      };
      
      this.log('JTAG', `Screenshot saved: ${filepath}`, metadata);
      return result;
      
    } catch (error: any) {
      this.critical('JTAG_SCREENSHOT', 'Screenshot save failed', { error: error.message });
      return {
        success: false,
        filepath: '',
        filename: '',
        context: 'browser',
        timestamp: payload.timestamp || new Date().toISOString(),
        error: `Screenshot save failed: ${error.message}`
      };
    }
  }

  /**
   * Auto-initialize if not done
   */
  private static ensureInitialized(): void {
    if (!this.initialized) {
      this.initialize();
    }
    
    // Ensure directories exist when first accessing logging
    if (this.isServer && this.config.logDirectory) {
      try {
        const fs = require('fs');
        if (!fs.existsSync(this.config.logDirectory)) {
          fs.mkdirSync(this.config.logDirectory, { recursive: true });
        }
      } catch (error) {
        console.error('JTAG: Failed to create log directory:', error);
      }
    }
  }


  /**
   * Server-side file logging with proper platform.level structure
   * 
   * Creates files as requested:
   * - platform.level (e.g., browser.log, server.warn)
   * - platform.level.json (e.g., browser.log.json, server.error.json)
   * 
   * Uses templates to create files if they don't exist
   */
  private static writeServerLog(entry: JTAGLogEntry): void {
    try {
      const fs = require('fs');
      const path = require('path');

      if (!this.config.logDirectory) return;

      // Ensure log directory exists (create only if missing)
      if (!fs.existsSync(this.config.logDirectory)) {
        fs.mkdirSync(this.config.logDirectory, { recursive: true });
      }

      const platform = entry.context; // 'browser' or 'server'
      const level = entry.type; // 'log', 'warn', 'error', etc.
      
      // File paths: platform.level.txt and platform.level.json  
      const textLogFile = path.join(this.config.logDirectory, `${platform}.${level}.txt`);
      const jsonLogFile = path.join(this.config.logDirectory, `${platform}.${level}.json`);

      // Create text log file if it doesn't exist (using template)
      if (!fs.existsSync(textLogFile)) {
        this.createLogFileFromTemplate(textLogFile, platform, level, false);
      }

      // Create JSON log file if it doesn't exist (using template)
      if (!fs.existsSync(jsonLogFile)) {
        this.createLogFileFromTemplate(jsonLogFile, platform, level, true);
      }

      // Append to text log file
      const logLine = `[${entry.timestamp}] ${entry.component}: ${entry.message}${entry.data ? ` | ${JSON.stringify(entry.data)}` : ''}\n`;
      fs.appendFileSync(textLogFile, logLine);

      // Append to JSON log file (read, add entry, write back)
      try {
        const jsonContent = fs.readFileSync(jsonLogFile, 'utf8');
        const logData = JSON.parse(jsonContent);
        
        logData.entries.push({
          timestamp: entry.timestamp,
          component: entry.component,
          message: entry.message,
          data: entry.data,
          type: entry.type,
          context: entry.context
        });
        
        fs.writeFileSync(jsonLogFile, JSON.stringify(logData, null, 2));
      } catch (jsonError) {
        // If JSON file is corrupted, recreate it
        this.createLogFileFromTemplate(jsonLogFile, platform, level, true);
      }

    } catch (error) {
      console.error('JTAG: Server logging failed:', error);
    }
  }

  /**
   * Create log file from template
   */
  private static createLogFileFromTemplate(
    filePath: string, 
    platform: string, 
    level: string, 
    isJson: boolean
  ): void {
    try {
      const fs = require('fs');
      const path = require('path');
      
      const templateFile = isJson ? 'log-template.json' : 'log-template.txt';
      const templatePath = path.join(__dirname, '../templates', templateFile);
      
      let template = '';
      
      // Try to load template, fallback to inline if not found
      try {
        template = fs.readFileSync(templatePath, 'utf8');
      } catch (templateError) {
        // Fallback templates
        if (isJson) {
          template = `{
  "meta": {
    "platform": "{PLATFORM}",
    "level": "{LEVEL}",
    "sessionId": "{SESSION_ID}",
    "created": "{TIMESTAMP}",
    "format": "JTAG structured log entries"
  },
  "entries": []
}`;
        } else {
          template = `# JTAG {PLATFORM}.{LEVEL} Log File
# Generated: {TIMESTAMP}
# Platform: {PLATFORM} (browser/server)
# Level: {LEVEL} (log/warn/error/info/critical/trace/probe)
# Session: {SESSION_ID}
# 
# Format: [timestamp] component: message | data
#
`;
        }
      }
      
      // Replace template variables
      const content = template
        .replace(/\{PLATFORM\}/g, platform)
        .replace(/\{LEVEL\}/g, level)
        .replace(/\{SESSION_ID\}/g, this.sessionId)
        .replace(/\{TIMESTAMP\}/g, new Date().toISOString());
      
      fs.writeFileSync(filePath, content);
      
    } catch (error) {
      console.error('JTAG: Template creation failed:', error);
    }
  }

  /**
   * Message queue for when WebSocket is disconnected
   */
  private static messageQueue: Array<{type: 'log' | 'screenshot' | 'exec', payload: any}> = [];
  private static readonly MAX_QUEUE_SIZE = 100;

  /**
   * Client-side remote logging with queuing support
   */
  private static sendToServer(entry: JTAGLogEntry): void {
    try {
      if (typeof window !== 'undefined') {
        // Try configured transport first, queue if disconnected
        this.sendTransportMessage('log', entry).catch((_error) => {
          // Queue message if transport is disconnected
          this.queueMessage('log', entry);
        });
      }
    } catch (error) {
      // Queue message on any error
      this.queueMessage('log', entry);
    }
  }

  /**
   * Queue message when WebSocket is disconnected
   */
  private static queueMessage(type: 'log' | 'screenshot' | 'exec', payload: any): void {
    if (this.messageQueue.length >= this.MAX_QUEUE_SIZE) {
      // Remove oldest message to make room
      this.messageQueue.shift();
    }
    
    this.messageQueue.push({ type, payload });
    
    // Use original console to avoid infinite loops
    if (this.consoleOriginals.log) {
      this.consoleOriginals.log('🔄 JTAG: Queued message (WebSocket disconnected), queue size:', this.messageQueue.length);
    }
  }

  /**
   * Flush queued messages when WebSocket reconnects
   */
  private static async flushQueuedMessages(): Promise<void> {
    if (this.messageQueue.length === 0) return;

    const originalQueueLength = this.messageQueue.length;
    
    // Use original console to avoid loops during flush
    if (this.consoleOriginals.log) {
      this.consoleOriginals.log('🚀 JTAG: Flushing', originalQueueLength, 'queued messages...');
    }

    // Process all queued messages
    const messages = [...this.messageQueue];
    this.messageQueue = []; // Clear queue

    for (const message of messages) {
      try {
        await this.sendTransportMessage(message.type, message.payload);
      } catch (error) {
        // If message fails during flush, re-queue it
        this.queueMessage(message.type, message.payload);
      }
    }

    if (this.consoleOriginals.log) {
      this.consoleOriginals.log('✅ JTAG: Flushed queued messages, remaining queue size:', this.messageQueue.length);
    }
  }

  /**
   * Initialize WebSocket server for client-server communication
   */
  private static async startServerLogging(): Promise<void> {
    if (this.serverStarted) {
      return; // Server already started
    }

    try {
      this.webSocketServer = new JTAGWebSocketServer({
        port: this.config.jtagPort,
        onLog: (entry: JTAGLogEntry) => {
          this.writeServerLog(entry);
        },
        onScreenshot: async (payload: any) => {
          return this.handleScreenshotPayload(payload);
        },
        onExec: async (code: string, options: JTAGExecOptions) => {
          return this.executeServerCode(code, options);
        }
      });
      
      await this.webSocketServer.start();
      this.serverStarted = true;
      
      // Emit READY status when WebSocket server successfully starts
      this.emitStatusEvent(JTAG_STATUS.READY, JTAG_TRANSPORT.WEBSOCKET, {
        wsState: 1, // WebSocket.OPEN
        reason: 'websocket_server_started'
      });
      
    } catch (error) {
      console.error('JTAG: WebSocket server startup failed:', error);
      
      // Emit ERROR status on server startup failure
      this.emitStatusEvent(JTAG_STATUS.ERROR, JTAG_TRANSPORT.WEBSOCKET, {
        error: error instanceof Error ? error.message : String(error),
        reason: 'websocket_server_failed'
      });
    }
  }

  /**
   * Initialize client-side remote logging
   */
  private static async initializeRemoteLogging(): Promise<void> {
    console.log('🔧 JTAG Browser: Starting initializeRemoteLogging');
    console.log('🔧 JTAG Browser: Context check - isClient:', this.isClient, 'isServer:', this.isServer);
    console.log('🔧 JTAG Browser: Config enableRemoteLogging:', this.config.enableRemoteLogging);
    console.log('🔧 JTAG Browser: JTAG Port:', this.config.jtagPort);
    
    // Add delay to allow server to fully start up
    setTimeout(async () => {
      console.log('🔧 JTAG Browser: Timeout reached, starting WebSocket connection...');
      
      try {
        console.log('🔧 JTAG Browser: Creating JTAGWebSocketClient with port:', this.config.jtagPort || 9001);
        
        // Initialize and connect WebSocket client for browser-to-server communication
        this.webSocketClient = new JTAGWebSocketClient(this.config.jtagPort || 9001);
        console.log('🔧 JTAG Browser: JTAGWebSocketClient created, calling connect...');
        
        await this.webSocketClient.connect();
        console.log('🔧 JTAG Browser: WebSocket connected successfully!');
        
        // Emit READY status when client WebSocket successfully connects
        this.emitStatusEvent(JTAG_STATUS.READY, JTAG_TRANSPORT.WEBSOCKET, {
          wsState: 1, // WebSocket.OPEN
          reason: 'websocket_client_connected'
        });
        
        // Flush any queued messages now that we're connected
        await this.flushQueuedMessages();
        
        this.log('JTAG', 'Browser WebSocket client connected', { 
          serverPort: this.config.jtagPort 
        });
        console.log('🔧 JTAG Browser: Log message sent after connection');
      } catch (error: any) {
        // Log detailed error information
        console.error('🔧 JTAG Browser: WebSocket connection failed with error:', error);
        console.error('🔧 JTAG Browser: Error message:', error.message);
        console.error('🔧 JTAG Browser: Error stack:', error.stack);
        console.warn('🌐 JTAG WebSocket client connection failed:', error.message);
        
        // Emit ERROR status on client connection failure
        this.emitStatusEvent(JTAG_STATUS.ERROR, JTAG_TRANSPORT.WEBSOCKET, {
          error: error.message,
          reason: 'websocket_client_failed'
        });
      }
    }, 2000); // Wait 2 seconds for server to be ready
  }

  /**
   * Clear all logs
   */
  static clearLogs(): void {
    this.logBuffer = [];
    
    if (this.isServer && this.config.logDirectory) {
      try {
        const fs = require('fs');
        const files = fs.readdirSync(this.config.logDirectory);
        for (const file of files) {
          if (file.includes('emergency')) {
            const path = require('path');
            fs.unlinkSync(path.join(this.config.logDirectory, file));
          }
        }
        console.log('🚨 Emergency JTAG: Logs cleared');
      } catch (error) {
        console.error('Emergency JTAG: Clear logs failed:', error);
      }
    }
  }

  /**
   * Get diagnostic statistics
   */
  static getStats(): JTAGStats {
    const entriesByType: Record<string, number> = {};
    const entriesByComponent: Record<string, number> = {};

    for (const entry of this.logBuffer) {
      entriesByType[entry.type] = (entriesByType[entry.type] || 0) + 1;
      entriesByComponent[entry.component] = (entriesByComponent[entry.component] || 0) + 1;
    }

    return {
      totalEntries: this.logBuffer.length,
      entriesByType,
      entriesByComponent,
      oldestEntry: this.logBuffer[0]?.timestamp,
      newestEntry: this.logBuffer[this.logBuffer.length - 1]?.timestamp
    };
  }

  /**
   * Get current log buffer
   */
  static getLogs(): JTAGLogEntry[] {
    return [...this.logBuffer];
  }

  /**
   * Process log entry - write to file, console, buffer
   */
  private static processLogEntry(entry: JTAGLogEntry): void {
    // Console output using ORIGINAL console to prevent recursion
    if (this.config.enableConsoleOutput) {
      const emoji = entry.type === 'critical' ? '🔥' : entry.type === 'trace' ? '🔍' : entry.type === 'probe' ? '📊' : entry.type === 'test' ? '🧪' : '📝';
      // Use original console.log to prevent infinite recursion
      this.consoleOriginals.log?.(`🚨 JTAG ${emoji} [${entry.component}]: ${entry.message}${entry.data ? ' ' + JSON.stringify(entry.data) : ''}`);
    }

    // Context-specific handling
    if (this.isServer) {
      this.writeServerLog(entry);
    } else if (this.isClient && this.config.enableRemoteLogging) {
      this.sendToServer(entry);
    }
    
    // Add to buffer
    this.logBuffer.push(entry);
    if (this.logBuffer.length > this.config.maxBufferSize) {
      this.logBuffer = this.logBuffer.slice(-Math.floor(this.config.maxBufferSize / 2));
    }
  }
}