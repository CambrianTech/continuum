/**
 * JTAG WebSocket Module
 * 
 * Extracted WebSocket logic for clean separation of concerns.
 * Handles both server-side WebSocket server and client-side WebSocket connections.
 */

import { JTAGLogEntry, JTAGScreenshotResult, JTAGExecOptions, JTAGExecResult } from './JTAGTypes';
import { jtagConfig } from './config';

export interface JTAGWebSocketMessage {
  type: 'log' | 'screenshot' | 'exec';
  payload: any;
  timestamp?: string;
  uuid?: string;
}

export interface JTAGWebSocketResponse {
  success: boolean;
  error?: string;
  data?: any;
  timestamp: string;
}

export interface JTAGWebSocketServerOptions {
  port: number;
  onLog?: (entry: JTAGLogEntry) => void;
  onScreenshot?: (payload: JTAGScreenshotPayload) => Promise<JTAGScreenshotResult>;
  onExec?: (code: string, options: JTAGExecOptions) => Promise<JTAGExecResult>;
}

export interface JTAGScreenshotPayload {
  filename: string;
  dataUrl: string;
  format: string;
  metadata: {
    width: number;
    height: number;
    size: number;
    selector?: string;
  };
  timestamp: string;
}

export interface JTAGExecPayload {
  code: string;
  options: JTAGExecOptions;
}

export class JTAGWebSocketServer {
  private wss: any = null;
  private serverStarted = false;
  private options: JTAGWebSocketServerOptions;
  private logHandler: (entry: JTAGLogEntry) => void;
  private screenshotHandler: (payload: JTAGScreenshotPayload) => Promise<JTAGScreenshotResult>;
  private execHandler: (code: string, options: JTAGExecOptions) => Promise<JTAGExecResult>;

  constructor(options: JTAGWebSocketServerOptions) {
    this.options = options;
    this.logHandler = options.onLog || this.defaultLogHandler;
    this.screenshotHandler = options.onScreenshot || this.defaultScreenshotHandler;
    this.execHandler = options.onExec || this.defaultExecHandler;
  }

  /**
   * Start WebSocket server
   */
  async start(): Promise<void> {
    if (this.serverStarted) {
      return; // Server already started
    }

    try {
      const WebSocket = require('ws');
      
      this.wss = new WebSocket.WebSocketServer({ port: this.options.port });
      
      this.wss.on('connection', (ws: any) => {
        this.log('JTAG_WS', 'Client connected to WebSocket');
        
        ws.on('message', (message: any) => {
          this.handleMessage(message, ws);
        });
        
        ws.on('close', () => {
          this.log('JTAG_WS', 'Client disconnected from WebSocket');
        });
        
        ws.on('error', (error: any) => {
          this.logError('JTAG_WS', 'WebSocket error', { error: error.message });
        });
      });

      this.wss.on('error', (error: any) => {
        if (error.code === 'EADDRINUSE') {
          console.log(`🚨 JTAG WebSocket port ${this.options.port} already in use - reusing existing server`);
          this.serverStarted = true;
        } else {
          console.error('JTAG: WebSocket server error:', error.message);
          throw error;
        }
      });

      this.wss.on('listening', () => {
        this.serverStarted = true;
        console.log(`🚨 JTAG WebSocket server listening on port ${this.options.port}`);
        console.log(`   📝 Logs: WebSocket messages`);
        console.log(`   📸 Screenshots: WebSocket messages`);
        console.log(`   📦 Payloads: WebSocket messages`);
        console.log(`   ⚡ Code Execution: WebSocket messages`);
      });

    } catch (error) {
      console.error('JTAG: WebSocket server startup failed:', error);
      throw error;
    }
  }

  /**
   * Stop WebSocket server
   */
  async stop(): Promise<void> {
    if (this.wss && this.serverStarted) {
      return new Promise((resolve) => {
        this.wss.close(() => {
          this.serverStarted = false;
          this.wss = null;
          resolve();
        });
      });
    }
  }

  /**
   * Check if server is running
   */
  isRunning(): boolean {
    return this.serverStarted && this.wss !== null;
  }

  /**
   * Get active connection count
   */
  getConnectionCount(): number {
    if (!this.wss) return 0;
    return this.wss.clients.size;
  }

  /**
   * Handle incoming WebSocket messages
   */
  private async handleMessage(message: any, ws: any): Promise<void> {
    try {
      const data: JTAGWebSocketMessage = JSON.parse(message.toString());
      this.validateMessage(data);
      
      const { type, payload } = data;
      let response: JTAGWebSocketResponse;
      
      switch (type) {
        case 'log':
          response = await this.handleLogMessage(payload as JTAGLogEntry);
          break;
          
        case 'screenshot':
          response = await this.handleScreenshotMessage(payload as JTAGScreenshotPayload);
          break;
          
        case 'exec':
          response = await this.handleExecMessage(payload as JTAGExecPayload);
          break;
          
        default:
          response = {
            success: false,
            error: `Unknown message type: ${type}`,
            timestamp: new Date().toISOString()
          };
      }
      
      ws.send(JSON.stringify(response));
    } catch (error: any) {
      this.logError('JTAG_WS', 'Invalid WebSocket message', { error: error.message });
      ws.send(JSON.stringify({
        success: false,
        error: `Message processing failed: ${error.message}`,
        timestamp: new Date().toISOString()
      }));
    }
  }

  /**
   * Validate WebSocket message structure
   */
  private validateMessage(data: any): void {
    if (!data || typeof data !== 'object') {
      throw new Error('Invalid message format: must be JSON object');
    }
    
    if (!data.type || typeof data.type !== 'string') {
      throw new Error('Invalid message format: missing or invalid type field');
    }
    
    if (!data.payload) {
      throw new Error('Invalid message format: missing payload field');
    }
    
    const validTypes = ['log', 'screenshot', 'exec'];
    if (!validTypes.includes(data.type)) {
      throw new Error(`Invalid message type: ${data.type}. Valid types: ${validTypes.join(', ')}`);
    }
  }

  /**
   * Handle log messages
   */
  private async handleLogMessage(payload: JTAGLogEntry): Promise<JTAGWebSocketResponse> {
    try {
      this.logHandler(payload);
      return {
        success: true,
        timestamp: new Date().toISOString()
      };
    } catch (error: any) {
      return {
        success: false,
        error: `Log processing failed: ${error.message}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Handle screenshot messages
   */
  private async handleScreenshotMessage(payload: JTAGScreenshotPayload): Promise<JTAGWebSocketResponse> {
    try {
      const result = await this.screenshotHandler(payload);
      return {
        success: result.success,
        data: result,
        ...(result.error && { error: result.error }),
        timestamp: new Date().toISOString()
      };
    } catch (error: any) {
      return {
        success: false,
        error: `Screenshot processing failed: ${error.message}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Handle code execution messages
   */
  private async handleExecMessage(payload: JTAGExecPayload): Promise<JTAGWebSocketResponse> {
    try {
      const result = await this.execHandler(payload.code, payload.options);
      return {
        success: result.success,
        data: result,
        ...(result.error && { error: result.error }),
        timestamp: new Date().toISOString()
      };
    } catch (error: any) {
      return {
        success: false,
        error: `Code execution failed: ${error.message}`,
        timestamp: new Date().toISOString()
      };
    }
  }

  /**
   * Default log handler
   */
  private defaultLogHandler(entry: JTAGLogEntry): void {
    console.log(`[${entry.timestamp}] ${entry.component}: ${entry.message}`, entry.data || '');
  }

  /**
   * Default screenshot handler
   */
  private async defaultScreenshotHandler(payload: JTAGScreenshotPayload): Promise<JTAGScreenshotResult> {
    // Extract base64 data from data URL
    const base64Data = payload.dataUrl.split(',')[1];
    const buffer = Buffer.from(base64Data, 'base64');
    
    // Ensure screenshot directory exists
    const fs = require('fs');
    if (!fs.existsSync(jtagConfig.screenshotDirectory)) {
      fs.mkdirSync(jtagConfig.screenshotDirectory, { recursive: true });
    }
    
    // Save image file
    const extension = payload.format === 'jpeg' ? 'jpg' : payload.format;
    const filepath = `${jtagConfig.screenshotDirectory}/${payload.filename}.${extension}`;
    fs.writeFileSync(filepath, buffer);
    
    return {
      success: true,
      filepath,
      filename: `${payload.filename}.${extension}`,
      context: 'browser',
      timestamp: payload.timestamp,
      metadata: {
        ...payload.metadata,
        size: buffer.length
      }
    };
  }

  /**
   * Default code execution handler
   */
  private async defaultExecHandler(_code: string, _options: JTAGExecOptions): Promise<JTAGExecResult> {
    // This is a placeholder - the actual execution logic would be injected
    throw new Error('Code execution handler not implemented');
  }

  /**
   * Utility logging methods
   */
  private log(component: string, message: string, data?: any): void {
    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context: 'server',
      component,
      message,
      data,
      type: 'log'
    };
    this.logHandler(entry);
  }

  private logError(component: string, message: string, data?: any): void {
    const entry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context: 'server',
      component,
      message,
      data,
      type: 'critical'
    };
    this.logHandler(entry);
  }
}

export class JTAGWebSocketClient {
  private websocket: WebSocket | null = null;
  private websocketConnecting = false;
  private port: number;
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000; // 1 second

  constructor(port: number = 9001) {
    this.port = port;
  }

  /**
   * Connect to WebSocket server with retry logic
   */
  async connect(): Promise<WebSocket> {
    console.log('🔧 JTAG WebSocket Client: connect() called');
    console.log('🔧 JTAG WebSocket Client: window check:', typeof window !== 'undefined');
    
    if (typeof window === 'undefined') {
      const error = new Error('WebSocket client only available in browser context');
      console.error('🔧 JTAG WebSocket Client: Not in browser context!', error);
      throw error;
    }

    console.log('🔧 JTAG WebSocket Client: Current websocket state:', this.websocket?.readyState);
    console.log('🔧 JTAG WebSocket Client: WebSocket.OPEN constant:', WebSocket.OPEN);

    if (this.websocket?.readyState === WebSocket.OPEN) {
      console.log('🔧 JTAG WebSocket Client: Already connected, returning existing websocket');
      return this.websocket;
    }

    if (this.websocketConnecting) {
      console.log('🔧 JTAG WebSocket Client: Connection in progress, waiting...');
      // Wait for existing connection attempt
      return this.waitForConnection();
    }

    console.log('🔧 JTAG WebSocket Client: Starting new connection attempt...');
    this.websocketConnecting = true;
    return this.attemptConnection();
  }

  /**
   * Send message via WebSocket with promise-based response handling
   */
  async sendMessage(message: JTAGWebSocketMessage): Promise<JTAGWebSocketResponse> {
    const ws = await this.connect();
    
    return new Promise((resolve, reject) => {
      const messageStr = JSON.stringify({
        ...message,
        timestamp: message.timestamp || new Date().toISOString(),
        uuid: message.uuid || this.generateUUID()
      });
      
      const timeout = setTimeout(() => {
        ws.removeEventListener('message', onMessage);
        reject(new Error('WebSocket message timeout'));
      }, 30000); // 30 second timeout
      
      const onMessage = (event: MessageEvent) => {
        try {
          const response: JTAGWebSocketResponse = JSON.parse(event.data);
          ws.removeEventListener('message', onMessage);
          clearTimeout(timeout);
          
          if (response.success) {
            resolve(response);
          } else {
            reject(new Error(response.error || 'Server response failed'));
          }
        } catch (error) {
          ws.removeEventListener('message', onMessage);
          clearTimeout(timeout);
          reject(error);
        }
      };

      ws.addEventListener('message', onMessage);
      ws.send(messageStr);
    });
  }

  /**
   * Send log message
   */
  async sendLog(entry: JTAGLogEntry): Promise<JTAGWebSocketResponse> {
    return this.sendMessage({
      type: 'log',
      payload: entry
    });
  }

  /**
   * Send screenshot data
   */
  async sendScreenshot(payload: JTAGScreenshotPayload): Promise<JTAGWebSocketResponse> {
    return this.sendMessage({
      type: 'screenshot',
      payload
    });
  }

  /**
   * Send code execution request
   */
  async sendExec(code: string, options: JTAGExecOptions = {}): Promise<JTAGWebSocketResponse> {
    return this.sendMessage({
      type: 'exec',
      payload: { code, options }
    });
  }

  /**
   * Disconnect from WebSocket server
   */
  disconnect(): void {
    if (this.websocket) {
      this.websocket.close();
      this.websocket = null;
    }
    this.websocketConnecting = false;
    this.reconnectAttempts = 0;
  }

  /**
   * Check connection status
   */
  isConnected(): boolean {
    return this.websocket?.readyState === WebSocket.OPEN;
  }

  /**
   * Wait for existing connection attempt
   */
  private waitForConnection(): Promise<WebSocket> {
    return new Promise((resolve, reject) => {
      const checkConnection = () => {
        if (this.websocket?.readyState === WebSocket.OPEN) {
          resolve(this.websocket);
        } else if (!this.websocketConnecting) {
          reject(new Error('WebSocket connection failed'));
        } else {
          setTimeout(checkConnection, 100);
        }
      };
      checkConnection();
    });
  }

  /**
   * Attempt WebSocket connection with retry logic
   */
  private attemptConnection(): Promise<WebSocket> {
    return new Promise((resolve, reject) => {
      try {
        const wsUrl = `ws://localhost:${this.port}`;
        console.log('🔧 JTAG WebSocket Client: Attempting connection to:', wsUrl);
        console.log('🔧 JTAG WebSocket Client: Current readyState before create:', this.websocket?.readyState);
        
        this.websocket = new WebSocket(wsUrl);
        console.log('🔧 JTAG WebSocket Client: WebSocket object created, readyState:', this.websocket.readyState);
        
        this.websocket.onopen = (event) => {
          console.log('🔧 JTAG WebSocket Client: onopen event fired', event);
          this.websocketConnecting = false;
          this.reconnectAttempts = 0;
          console.log('🚨 JTAG WebSocket connected successfully to port', this.port);
          console.log('🔧 JTAG WebSocket Client: Final readyState on open:', this.websocket?.readyState);
          resolve(this.websocket!);
        };

        this.websocket.onerror = (error: Event) => {
          console.error('🔧 JTAG WebSocket Client: onerror event fired:', error);
          console.error('🔧 JTAG WebSocket Client: Error type:', error.type);
          console.error('🔧 JTAG WebSocket Client: WebSocket readyState on error:', this.websocket?.readyState);
          console.error('🚨 JTAG WebSocket connection error details:', {
            port: this.port,
            url: wsUrl,
            readyState: this.websocket?.readyState,
            error: error
          });
          this.attemptReconnect(reject);
        };

        this.websocket.onclose = (event: CloseEvent) => {
          console.log('🔧 JTAG WebSocket Client: onclose event fired');
          console.log('🔧 JTAG WebSocket Client: Close code:', event.code);
          console.log('🔧 JTAG WebSocket Client: Close reason:', event.reason);
          console.log('🔧 JTAG WebSocket Client: Was clean:', event.wasClean);
          console.log('🚨 JTAG WebSocket disconnected:', event.reason, 'code:', event.code);
          this.websocket = null;
          this.websocketConnecting = false;
          
          // Attempt reconnection if not a normal close
          if (event.code !== 1000 && this.reconnectAttempts < this.maxReconnectAttempts) {
            console.log('🔧 JTAG WebSocket Client: Attempting reconnect due to abnormal close');
            this.attemptReconnect();
          } else {
            console.log('🔧 JTAG WebSocket Client: Not attempting reconnect - normal close or max attempts reached');
          }
        };

        console.log('🔧 JTAG WebSocket Client: Event listeners attached, waiting for connection...');

      } catch (error) {
        console.error('🔧 JTAG WebSocket Client: Exception in attemptConnection:', error);
        this.websocketConnecting = false;
        reject(error);
      }
    });
  }

  /**
   * Handle reconnection logic
   */
  private attemptReconnect(rejectCallback?: (reason: any) => void): void {
    this.websocketConnecting = false;
    
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      const error = new Error(`Failed to connect after ${this.maxReconnectAttempts} attempts`);
      if (rejectCallback) {
        rejectCallback(error);
      }
      return;
    }

    this.reconnectAttempts++;
    const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1); // Exponential backoff
    
    console.log(`🚨 JTAG WebSocket reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`);
    
    setTimeout(() => {
      if (this.reconnectAttempts < this.maxReconnectAttempts) {
        this.connect().catch(() => {
          // Reconnection will be handled by the onError/onClose handlers
        });
      }
    }, delay);
  }

  /**
   * Generate unique UUID for messages
   */
  private generateUUID(): string {
    return 'ws_' + Date.now().toString(36) + '_' + Math.random().toString(36).substr(2, 9);
  }
}

/**
 * Utility functions for WebSocket message handling
 */
export class JTAGWebSocketUtils {
  /**
   * Create a log message
   */
  static createLogMessage(component: string, message: string, data?: any, type: 'log' | 'critical' | 'trace' | 'probe' = 'log'): JTAGWebSocketMessage {
    return {
      type: 'log',
      payload: {
        timestamp: new Date().toISOString(),
        context: typeof window !== 'undefined' ? 'browser' : 'server',
        component,
        message,
        data,
        type
      } as JTAGLogEntry
    };
  }

  /**
   * Create a screenshot message
   */
  static createScreenshotMessage(filename: string, dataUrl: string, format: string, metadata: any): JTAGWebSocketMessage {
    return {
      type: 'screenshot',
      payload: {
        filename,
        dataUrl,
        format,
        metadata,
        timestamp: new Date().toISOString()
      } as JTAGScreenshotPayload
    };
  }

  /**
   * Create an execution message
   */
  static createExecMessage(code: string, options: JTAGExecOptions = {}): JTAGWebSocketMessage {
    return {
      type: 'exec',
      payload: {
        code,
        options
      } as JTAGExecPayload
    };
  }

  /**
   * Validate WebSocket response
   */
  static validateResponse(response: any): JTAGWebSocketResponse {
    if (!response || typeof response !== 'object') {
      throw new Error('Invalid response format');
    }
    
    if (typeof response.success !== 'boolean') {
      throw new Error('Invalid response: missing success field');
    }
    
    if (!response.timestamp) {
      throw new Error('Invalid response: missing timestamp field');
    }
    
    return response as JTAGWebSocketResponse;
  }
}