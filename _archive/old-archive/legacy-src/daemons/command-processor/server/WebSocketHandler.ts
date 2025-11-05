/**
 * WebSocket Handler - Focused daemon for WebSocket message handling
 * 
 * Extracted from CommandProcessorDaemon as part of symmetric architecture migration.
 * Handles WebSocket-specific message processing, real-time communication, and
 * bidirectional command execution coordination.
 * 
 * Responsibilities:
 * - Process WebSocket messages and real-time communication
 * - Handle bidirectional command execution (browser ↔ server)
 * - Manage WebSocket session state and connection lifecycle
 * - Transform WebSocket messages into standardized command messages
 */

import { BaseDaemon } from '../../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../../base/DaemonProtocol';
import { DaemonType } from '../../base/DaemonTypes';
import { 
  isExecuteCommandMessage
} from '../shared';

// ✅ WEBSOCKET MESSAGE CONTEXT
export interface WebSocketContext {
  readonly connectionId: string;
  readonly clientType: 'browser' | 'cli' | 'mobile' | 'desktop';
  readonly sessionId?: string;
  readonly timestamp: Date;
  readonly metadata?: Record<string, any>;
}

// ✅ WEBSOCKET VALIDATION RESULT
export interface WebSocketValidationResult {
  readonly success: boolean;
  readonly command?: string;
  readonly parameters?: unknown;
  readonly context?: WebSocketContext;
  readonly error?: string;
  readonly messageId?: string;
}

// ✅ WEBSOCKET RESPONSE FORMAT
export interface WebSocketResponse {
  readonly messageId: string;
  readonly type: 'command_response' | 'command_error' | 'status_update';
  readonly success: boolean;
  readonly data?: any;
  readonly error?: string;
  readonly timestamp: Date;
}

export class WebSocketHandler extends BaseDaemon {
  public readonly name = 'websocket-handler';
  public readonly version = '1.0.0';
  public readonly id: string;
  public readonly daemonType = DaemonType.COMMAND_PROCESSOR; // Will create new type later
  public readonly config = {
    name: this.name,
    version: this.version,
    port: 9005, // Different port from other daemons
    autoStart: true,
    dependencies: ['command-router'],
    healthCheck: { interval: 30000, timeout: 5000, retries: 3 },
    resources: { maxMemory: 256, maxCpu: 40 }
  };

  private readonly activeConnections = new Map<string, WebSocketContext>();
  private readonly messageHistory: Array<{ context: WebSocketContext; message: any; timestamp: Date }> = [];

  constructor() {
    super();
    this.id = `${this.name}-${Date.now()}`;
  }

  protected async onStart(): Promise<void> {
    this.log(`🚀 Starting ${this.name} daemon`);
  }

  protected async onStop(): Promise<void> {
    this.log(`🛑 Stopping ${this.name} daemon`);
    this.activeConnections.clear();
  }

  getMessageTypes(): string[] {
    return [
      'execute_command',        // Main WebSocket command execution
      'websocket.connect',      // Connection lifecycle
      'websocket.disconnect',   // Disconnection handling
      'websocket.validate',     // Message validation
      'websocket.broadcast'     // Broadcast to multiple connections
    ];
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    this.log(`🔌 WEBSOCKET: Handling message type: ${message.type}`);
    
    try {
      switch (message.type) {
        case 'execute_command':
          return await this.handleExecuteCommand(message);
          
        case 'websocket.connect':
          return await this.handleConnection(message);
          
        case 'websocket.disconnect':
          return await this.handleDisconnection(message);
          
        case 'websocket.validate':
          return await this.handleMessageValidation(message);
          
        case 'websocket.broadcast':
          return await this.handleBroadcast(message);
          
        default:
          return {
            success: false,
            error: `Unsupported message type: ${message.type}`
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ WEBSOCKET: Error handling message: ${errorMessage}`);
      
      return {
        success: false,
        error: `WebSocket handling failed: ${errorMessage}`
      };
    }
  }

  /**
   * Handle WebSocket command execution
   */
  private async handleExecuteCommand(message: DaemonMessage): Promise<DaemonResponse> {
    if (!isExecuteCommandMessage(message)) {
      return this.createWebSocketErrorResponse('Invalid execute_command message format', message.id);
    }

    const execData = message.data;
    this.log(`⚡ WEBSOCKET: Processing command ${execData.command}`);

    // Validate WebSocket message
    const validation = this.validateWebSocketMessage(execData, message);
    if (!validation.success) {
      return this.createWebSocketErrorResponse(validation.error!, validation.messageId);
    }

    // Transform to command execution format
    const commandMessage = this.transformWebSocketToCommand(validation);
    
    // Route to command execution through router
    return await this.routeToCommandExecution(commandMessage, validation.context!);
  }

  /**
   * Handle WebSocket connection
   */
  private async handleConnection(message: DaemonMessage): Promise<DaemonResponse> {
    const connectionData = message.data as any;
    const connectionId = connectionData?.connectionId || `ws-${Date.now()}`;
    
    const context: WebSocketContext = {
      connectionId,
      clientType: connectionData?.clientType || 'browser',
      sessionId: connectionData?.sessionId,
      timestamp: new Date(),
      metadata: connectionData?.metadata
    };

    this.activeConnections.set(connectionId, context);
    this.log(`🔗 WEBSOCKET: New connection ${connectionId} (${context.clientType})`);

    return {
      success: true,
      data: {
        connectionId,
        context,
        message: 'WebSocket connection established'
      }
    };
  }

  /**
   * Handle WebSocket disconnection
   */
  private async handleDisconnection(message: DaemonMessage): Promise<DaemonResponse> {
    const disconnectionData = message.data as any;
    const connectionId = disconnectionData?.connectionId;

    if (!connectionId) {
      return {
        success: false,
        error: 'Connection ID required for disconnection'
      };
    }

    const context = this.activeConnections.get(connectionId);
    this.activeConnections.delete(connectionId);
    
    this.log(`🔌 WEBSOCKET: Disconnection ${connectionId} ${context ? `(${context.clientType})` : '(unknown)'}`);

    return {
      success: true,
      data: {
        connectionId,
        context,
        message: 'WebSocket connection closed'
      }
    };
  }

  /**
   * Handle message validation
   */
  private async handleMessageValidation(message: DaemonMessage): Promise<DaemonResponse> {
    const wsData = message.data;
    const validation = this.validateWebSocketMessage(wsData, message);
    
    return {
      success: validation.success,
      data: validation,
      ...(validation.error && { error: validation.error })
    };
  }

  /**
   * Handle broadcast to multiple connections
   */
  private async handleBroadcast(message: DaemonMessage): Promise<DaemonResponse> {
    const broadcastData = message.data as any;
    const { targetConnections, payload, messageType } = broadcastData;
    
    // Suppress unused variable warnings for placeholder implementation
    void payload;
    void messageType;

    let broadcastCount = 0;
    const errors: string[] = [];

    // If no specific targets, broadcast to all active connections
    const targets = targetConnections || Array.from(this.activeConnections.keys());

    for (const connectionId of targets) {
      try {
        const context = this.activeConnections.get(connectionId);
        if (context) {
          // TODO: Implement actual WebSocket broadcasting
          this.log(`📡 WEBSOCKET: Broadcasting to ${connectionId}`);
          broadcastCount++;
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        errors.push(`${connectionId}: ${errorMessage}`);
      }
    }

    return {
      success: errors.length === 0,
      data: {
        broadcastCount,
        targetCount: targets.length,
        ...(errors.length > 0 && { errors })
      }
    };
  }

  /**
   * Validate WebSocket message format and extract command information
   */
  private validateWebSocketMessage(wsData: any, message: DaemonMessage): WebSocketValidationResult {
    if (!wsData || typeof wsData !== 'object') {
      return {
        success: false,
        error: 'WebSocket data is required and must be an object',
        messageId: message.id
      };
    }

    const { command, parameters } = wsData;

    if (!command) {
      return {
        success: false,
        error: 'Command is required in WebSocket message',
        messageId: message.id
      };
    }

    // Determine connection context from message metadata or defaults
    const connectionId = wsData.connectionId || message.from || `ws-${Date.now()}`;
    const existingContext = this.activeConnections.get(connectionId);

    const context: WebSocketContext = existingContext || {
      connectionId,
      clientType: wsData.clientType || 'browser',
      sessionId: wsData.sessionId,
      timestamp: new Date(),
      metadata: wsData.metadata
    };

    // Store context if new
    if (!existingContext) {
      this.activeConnections.set(connectionId, context);
    }

    return {
      success: true,
      command,
      parameters: parameters || {},
      context,
      messageId: message.id
    };
  }

  /**
   * Transform WebSocket message to command message
   */
  private transformWebSocketToCommand(validation: WebSocketValidationResult): DaemonMessage {
    return {
      id: `ws-cmd-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      type: 'command.route',
      from: this.name,
      to: 'command-router',
      data: {
        command: validation.command!,
        parameters: validation.parameters,
        context: {
          source: 'websocket',
          connectionId: validation.context!.connectionId,
          clientType: validation.context!.clientType,
          sessionId: validation.context!.sessionId,
          originalMessageId: validation.messageId
        }
      },
      timestamp: new Date()
    };
  }

  /**
   * Route command to execution through command router
   */
  private async routeToCommandExecution(commandMessage: DaemonMessage, wsContext: WebSocketContext): Promise<DaemonResponse> {
    this.log(`📤 WEBSOCKET: Routing command ${(commandMessage.data as any).command} to execution`);
    
    try {
      // Record message in history
      this.recordMessageHistory(wsContext, commandMessage);
      
      // TODO: Implement actual daemon-to-daemon communication
      // For now, return formatted WebSocket response indicating routing worked
      
      const wsResponse = this.formatWebSocketResponse({
        success: true,
        data: {
          message: 'Command routed for execution',
          command: (commandMessage.data as any).command,
          routed: true
        }
      }, commandMessage.id);
      
      return {
        success: true,
        data: wsResponse
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      const wsResponse = this.formatWebSocketResponse({
        success: false,
        error: errorMessage
      }, commandMessage.id);
      
      return {
        success: false,
        data: wsResponse,
        error: errorMessage
      };
    }
  }

  /**
   * Format response as WebSocket-compatible format
   */
  private formatWebSocketResponse(
    result: { success: boolean; data?: any; error?: string }, 
    messageId: string
  ): WebSocketResponse {
    return {
      messageId: messageId || `response-${Date.now()}`,
      type: result.success ? 'command_response' : 'command_error',
      success: result.success,
      data: result.data,
      ...(result.error && { error: result.error }),
      timestamp: new Date()
    };
  }

  /**
   * Create WebSocket error response
   */
  private createWebSocketErrorResponse(error: string, messageId?: string): DaemonResponse {
    const wsResponse = this.formatWebSocketResponse({
      success: false,
      error
    }, messageId || `error-${Date.now()}`);

    return {
      success: false,
      error,
      data: wsResponse
    };
  }

  /**
   * Record message in history for debugging and analytics
   */
  private recordMessageHistory(context: WebSocketContext, message: any): void {
    this.messageHistory.push({
      context,
      message,
      timestamp: new Date()
    });

    // Limit history size to prevent memory leaks
    if (this.messageHistory.length > 500) {
      this.messageHistory.splice(0, 100); // Remove oldest 100 entries
    }
  }

  /**
   * Get connection and message statistics
   */
  public getWebSocketStats(): {
    activeConnections: number;
    totalMessages: number;
    connectionsByType: Record<string, number>;
    avgMessagesPerConnection: number;
  } {
    const connectionsByType: Record<string, number> = {};
    
    for (const context of this.activeConnections.values()) {
      connectionsByType[context.clientType] = (connectionsByType[context.clientType] || 0) + 1;
    }

    return {
      activeConnections: this.activeConnections.size,
      totalMessages: this.messageHistory.length,
      connectionsByType,
      avgMessagesPerConnection: this.activeConnections.size > 0 ? 
        this.messageHistory.length / this.activeConnections.size : 0
    };
  }

  /**
   * Get active connections summary
   */
  public getActiveConnections(): Array<{
    connectionId: string;
    clientType: string;
    sessionId?: string;
    connectedSince: Date;
  }> {
    return Array.from(this.activeConnections.values()).map(context => ({
      connectionId: context.connectionId,
      clientType: context.clientType,
      ...(context.sessionId && { sessionId: context.sessionId }),
      connectedSince: context.timestamp
    }));
  }
}