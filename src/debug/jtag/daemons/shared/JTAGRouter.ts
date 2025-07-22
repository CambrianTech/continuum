/**
 * JTAG Symmetric Router
 * 
 * Dumb router that automatically adds /client and /server prefixes
 * to daemon endpoints based on execution context. Each daemon registers
 * once but gets both client and server routes.
 */

import { MessageSubscriber, DaemonMessage, DaemonResponse } from './MessageSubscriber';

export interface RouterContext {
  environment: 'client' | 'server' | 'universal';
  prefix?: string;
  transports?: Map<string, MessageTransport>;
  encoder?: PayloadEncoder;
}

/**
 * Transport layer handles actual message delivery (WebSocket, HTTP, MCP, etc.)
 */
export interface MessageTransport {
  name: string;
  send(message: DaemonMessage, endpoint: string): Promise<DaemonResponse>;
  canHandle(prefix: string): boolean;
  isAvailable(): boolean;
}

/**
 * Payload encoding handles message serialization/encoding (separate from transport)
 */
export interface PayloadEncoder {
  name: string;
  encode(payload: any): string;
  decode(encoded: string): any;
  canHandle(messageType: string): boolean;
}

/**
 * WebSocket Transport - real-time bidirectional communication
 */
export class WebSocketTransport implements MessageTransport {
  name = 'websocket';
  private client?: WebSocket;
  
  async send(message: DaemonMessage, endpoint: string): Promise<DaemonResponse> {
    if (!this.client || this.client.readyState !== WebSocket.OPEN) {
      throw new Error('WebSocket not connected');
    }
    
    return new Promise((resolve, reject) => {
      const messageId = Date.now().toString();
      
      const handler = (event: MessageEvent) => {
        const response = JSON.parse(event.data);
        if (response.messageId === messageId) {
          this.client!.removeEventListener('message', handler);
          resolve(response);
        }
      };
      
      this.client!.addEventListener('message', handler);
      this.client!.send(JSON.stringify({ ...message, messageId, endpoint }));
      
      setTimeout(() => {
        this.client!.removeEventListener('message', handler);
        reject(new Error('WebSocket timeout'));
      }, 5000);
    });
  }
  
  canHandle(prefix: string): boolean {
    return prefix.startsWith('/client/') || prefix.startsWith('/events/');
  }
  
  isAvailable(): boolean {
    return typeof WebSocket !== 'undefined';
  }
}

/**
 * HTTP Transport - REST API communication
 */
export class HTTPTransport implements MessageTransport {
  name = 'http';
  private baseUrl: string;
  
  constructor(baseUrl = 'http://localhost:9002') {
    this.baseUrl = baseUrl;
  }
  
  async send(message: DaemonMessage, endpoint: string): Promise<DaemonResponse> {
    const response = await fetch(`${this.baseUrl}/api${endpoint}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(message)
    });
    
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: ${response.statusText}`);
    }
    
    return await response.json();
  }
  
  canHandle(prefix: string): boolean {
    return prefix.startsWith('/http/') || prefix.startsWith('/remote/');
  }
  
  isAvailable(): boolean {
    return typeof fetch !== 'undefined';
  }
}

/**
 * AsyncQueue Transport - server-side internal communication
 */
export class AsyncQueueTransport implements MessageTransport {
  name = 'async-queue';
  
  async send(message: DaemonMessage, endpoint: string): Promise<DaemonResponse> {
    // For server-side internal routing, we can process immediately
    return {
      success: true,
      data: { processed: true, transport: 'async-queue' },
      timestamp: new Date().toISOString()
    };
  }
  
  canHandle(prefix: string): boolean {
    return prefix.startsWith('/server/');
  }
  
  isAvailable(): boolean {
    return typeof window === 'undefined'; // Server-side only
  }
}

/**
 * Base64 Encoder - prevents parse issues with special characters
 */
export class Base64Encoder implements PayloadEncoder {
  name = 'base64';
  
  encode(payload: any): string {
    const jsonString = JSON.stringify(payload);
    return Buffer.from(jsonString, 'utf-8').toString('base64');
  }
  
  decode(encoded: string): any {
    try {
      const jsonString = Buffer.from(encoded, 'base64').toString('utf-8');
      return JSON.parse(jsonString);
    } catch (error) {
      console.warn('Base64Encoder: Failed to decode:', error);
      return { error: 'Failed to decode', original: encoded };
    }
  }
  
  canHandle(messageType: string): boolean {
    return messageType.includes('/console') || messageType.includes('/events');
  }
}

/**
 * JSON Encoder - simple JSON serialization for clean payloads
 */
export class JSONEncoder implements PayloadEncoder {
  name = 'json';
  
  encode(payload: any): string {
    return JSON.stringify(payload);
  }
  
  decode(encoded: string): any {
    try {
      return JSON.parse(encoded);
    } catch (error) {
      console.warn('JSONEncoder: Failed to decode:', error);
      return { error: 'Failed to decode', original: encoded };
    }
  }
  
  canHandle(messageType: string): boolean {
    return !messageType.includes('/console') && !messageType.includes('/events');
  }
}

export class JTAGRouter {
  private subscribers = new Map<string, MessageSubscriber>();
  private uuidMap = new Map<string, MessageSubscriber>();
  private context: RouterContext;
  private transports = new Map<string, MessageTransport>();
  private defaultTransport: MessageTransport;
  private encoder: PayloadEncoder;

  constructor(context: RouterContext = { environment: 'universal' }) {
    this.context = context;
    
    // Initialize transports by route prefix
    this.defaultTransport = new AsyncQueueTransport();
    this.transports.set('/client/', new WebSocketTransport());   // Browser WebSocket
    this.transports.set('/server/', new AsyncQueueTransport()); // Server internal
    this.transports.set('/events/', new WebSocketTransport());  // Events via WebSocket
    this.transports.set('/remote/', new HTTPTransport());       // Remote via HTTP
    this.transports.set('/http/', new HTTPTransport());         // Explicit HTTP
    
    // Initialize encoder (base64 by default for safety)
    this.encoder = context.encoder || new Base64Encoder();
    
    // Use custom transports if provided
    if (context.transports) {
      context.transports.forEach((transport, prefix) => {
        this.transports.set(prefix, transport);
      });
    }
  }

  /**
   * Register a daemon - router automatically creates context routes
   * Daemon just specifies base endpoint like 'command' or 'console'
   */
  registerSubscriber(endpoint: string, subscriber: MessageSubscriber): void {
    const uuid = subscriber.getUUID();
    
    // Router creates all possible routes automatically
    const routes = [
      `/client/${endpoint}`,   // Local client context
      `/server/${endpoint}`,   // Local server context
      `/remote/${uuid}`,       // Remote UUID-based access
      `/${endpoint}`           // Direct base endpoint
    ];
    
    // Register all routes to the same daemon
    routes.forEach(route => {
      this.subscribers.set(route, subscriber);
    });
    
    // Register by UUID for direct access
    this.uuidMap.set(uuid, subscriber);
    
    console.log(`🔌 JTAG Router: Registered '${endpoint}' as:`);
    console.log(`   📱 Client: /client/${endpoint} (${this.transports.get('/client/')?.name})`);
    console.log(`   🖥️  Server: /server/${endpoint} (${this.transports.get('/server/')?.name})`);
    console.log(`   🌐 Remote: /remote/${uuid} (${this.transports.get('/remote/')?.name})`);
    console.log(`   🎯 Direct: /${endpoint}`);
    console.log(`   🔗 UUID: ${uuid}`);
  }

  /**
   * Set transport for specific route prefix
   */
  setTransport(prefix: string, transport: MessageTransport): void {
    this.transports.set(prefix, transport);
    console.log(`🚛 JTAG Router: Set ${transport.name} transport for '${prefix}'`);
  }
  
  /**
   * Set payload encoder
   */
  setEncoder(encoder: PayloadEncoder): void {
    this.encoder = encoder;
    console.log(`🔒 JTAG Router: Set ${encoder.name} encoder`);
  }
  
  /**
   * Get appropriate transport for message type
   */
  private getTransport(messageType: string): MessageTransport {
    // Find best matching transport
    for (const [prefix, transport] of this.transports.entries()) {
      if (messageType.startsWith(prefix) && transport.canHandle(messageType) && transport.isAvailable()) {
        return transport;
      }
    }
    
    // Fallback to default transport if available
    if (this.defaultTransport.isAvailable()) {
      return this.defaultTransport;
    }
    
    // Find any available transport
    for (const transport of this.transports.values()) {
      if (transport.isAvailable()) {
        return transport;
      }
    }
    
    throw new Error('No available transports');
  }
  
  /**
   * Encode message payload
   */
  private encodePayload(message: DaemonMessage): DaemonMessage & { _encoding?: string } {
    // Only encode if encoder can handle this message type
    if (this.encoder.canHandle(message.type)) {
      return {
        ...message,
        payload: this.encoder.encode(message.payload),
        _encoding: this.encoder.name
      };
    }
    
    return message;
  }
  
  /**
   * Decode message payload
   */
  private decodePayload(message: DaemonMessage & { _encoding?: string }): DaemonMessage {
    const encodingUsed = message._encoding;
    
    if (encodingUsed && encodingUsed !== 'json') {
      return {
        ...message,
        payload: this.encoder.decode(message.payload as string)
      };
    }
    
    return message;
  }
  
  /**
   * Route message to appropriate subscriber with automatic encoding/decoding
   */
  async routeMessage(message: DaemonMessage): Promise<DaemonResponse[]> {
    const results: DaemonResponse[] = [];
    
    // Try UUID-based routing first (direct access)
    if (message.target && this.uuidMap.has(message.target)) {
      const subscriber = this.uuidMap.get(message.target)!;
      const result = await subscriber.handleMessage(message);
      results.push(result);
      return results;
    }
    
    // Decode message payload before routing (if encoded)
    const decodedMessage = this.decodePayload(message as any);
    
    // Try endpoint-based routing
    const endpoint = this.resolveEndpoint(message.type);
    if (this.subscribers.has(endpoint)) {
      const subscriber = this.subscribers.get(endpoint)!;
      const result = await subscriber.handleMessage(decodedMessage);
      results.push(result);
    }
    
    // If no exact match, try both client and server variants
    if (results.length === 0) {
      await this.tryBothContexts(decodedMessage, results);
    }
    
    if (results.length === 0) {
      results.push({
        success: false,
        error: `No subscriber found for ${message.type}`,
        timestamp: new Date().toISOString()
      });
    }
    
    return results;
  }

  private async tryBothContexts(message: DaemonMessage, results: DaemonResponse[]): Promise<void> {
    const baseType = message.type.replace(/^\/?(client|server|remote)\//, '');
    
    // Try all possible routes for this message
    const possibleRoutes = [
      `/client/${baseType}`,
      `/server/${baseType}`,
      `/${baseType}`,
      // Also try if baseType is actually a UUID for /remote/ routing
      baseType.length > 20 ? `/remote/${baseType}` : null
    ].filter(Boolean) as string[];
    
    for (const route of possibleRoutes) {
      if (this.subscribers.has(route)) {
        const subscriber = this.subscribers.get(route)!;
        const result = await subscriber.handleMessage(message);
        results.push(result);
        
        // If we found a direct match, we can stop
        if (route === message.type) {
          break;
        }
      }
    }
  }

  private resolveEndpoint(messageType: string): string {
    // If message already has full route, use as-is
    if (messageType.startsWith('/client/') || 
        messageType.startsWith('/server/') || 
        messageType.startsWith('/remote/') ||
        messageType.startsWith('/')) {
      return messageType;
    }
    
    // Auto-add context prefix based on router environment
    const prefix = this.context.environment === 'client' ? '/client' : 
                   this.context.environment === 'server' ? '/server' : '';
    
    return prefix ? `${prefix}/${messageType}` : `/${messageType}`;
  }

  /**
   * Get all registered endpoints (for debugging)
   */
  getRegisteredEndpoints(): string[] {
    return Array.from(this.subscribers.keys());
  }

  /**
   * Get subscriber by UUID (for direct access)
   */
  getSubscriberByUUID(uuid: string): MessageSubscriber | undefined {
    return this.uuidMap.get(uuid);
  }

  /**
   * Check if router has business logic (should always be false - dumb router!)
   */
  hasBusinessLogic(): boolean {
    return false; // This router is dumb - only routes based on patterns
  }
}