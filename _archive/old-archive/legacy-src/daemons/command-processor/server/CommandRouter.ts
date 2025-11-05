/**
 * Command Router - Focused daemon for command message routing and extraction
 * 
 * Extracted from CommandProcessorDaemon as part of symmetric architecture migration.
 * Handles routing of command messages to appropriate executors while maintaining
 * session context and care validation.
 * 
 * Responsibilities:
 * - Route incoming daemon messages to command executors
 * - Extract command information from various message formats
 * - Maintain session context during command execution
 * - Provide consistent message transformation patterns
 */

import { BaseDaemon } from '../../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../../base/DaemonProtocol';
import { DaemonType } from '../../base/DaemonTypes';
import { 
  TypedCommandRequest,
  isCommandExecuteMessage,
  isExecuteCommandMessage,
  isHandleApiMessage,
  isHandleWidgetApiMessage
} from '../shared';

// ✅ COMMAND EXTRACTION RESULT
export interface CommandExtractionResult {
  readonly success: boolean;
  readonly command?: string;
  readonly parameters?: unknown;
  readonly context?: Record<string, any>;
  readonly continuumContext?: any;
  readonly error?: string;
  readonly messageType?: string;
}

// ✅ ROUTING DECISION
export interface RoutingDecision {
  readonly targetDaemon: 'command-executor' | 'http-api-handler' | 'websocket-handler';
  readonly transformedMessage: DaemonMessage;
  readonly requiresContext: boolean;
}

export class CommandRouter extends BaseDaemon {
  public readonly name = 'command-router';
  public readonly version = '1.0.0';
  public readonly id: string;
  public readonly daemonType = DaemonType.COMMAND_PROCESSOR; // Will create new type later
  public readonly config = {
    name: this.name,
    version: this.version,
    port: 9002, // Different port from main processor
    autoStart: true,
    dependencies: ['command-executor'],
    healthCheck: { interval: 30000, timeout: 5000, retries: 3 },
    resources: { maxMemory: 256, maxCpu: 50 }
  };

  constructor() {
    super();
    this.id = `${this.name}-${Date.now()}`;
  }

  protected async onStart(): Promise<void> {
    this.log(`🚀 Starting ${this.name} daemon`);
  }

  protected async onStop(): Promise<void> {
    this.log(`🛑 Stopping ${this.name} daemon`);
  }

  getMessageTypes(): string[] {
    return [
      'command.route',       // New: unified routing message
      'command.execute',     // Direct command execution 
      'execute_command',     // WebSocket command execution
      'handle_api',          // HTTP API routing
      'handle_widget_api'    // Widget API routing
    ];
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    this.log(`🎯 ROUTER: Handling message type: ${message.type}`);
    
    try {
      // Extract command information from the message
      const extraction = await this.extractCommandFromMessage(message);
      
      if (!extraction.success) {
        return {
          success: false,
          error: extraction.error || 'Failed to extract command information'
        };
      }

      // Determine routing strategy
      const routing = this.determineRouting(message, extraction);
      
      // Execute with appropriate context handling
      if (extraction.continuumContext) {
        return await this.executeWithContext(routing, extraction);
      } else {
        return await this.executeDirectly(routing, extraction);
      }
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ ROUTER: Error handling message: ${errorMessage}`);
      
      return {
        success: false,
        error: `Command routing failed: ${errorMessage}`
      };
    }
  }

  /**
   * Extract command information from various message formats
   */
  private async extractCommandFromMessage(message: DaemonMessage): Promise<CommandExtractionResult> {
    const messageType = message.type;
    
    switch (messageType) {
      case 'command.execute':
        return this.extractFromCommandExecute(message);
        
      case 'execute_command':
        return this.extractFromExecuteCommand(message);
        
      case 'handle_api':
        return this.extractFromHandleApi(message);
        
      case 'handle_widget_api':
        return this.extractFromHandleWidgetApi(message);
        
      default:
        return {
          success: false,
          error: `Unsupported message type: ${messageType}`,
          messageType
        };
    }
  }

  /**
   * Extract from direct command execution message
   */
  private extractFromCommandExecute(message: DaemonMessage): CommandExtractionResult {
    if (!isCommandExecuteMessage(message)) {
      return {
        success: false,
        error: 'Invalid command.execute message format',
        messageType: message.type
      };
    }

    const directData = message.data;
    const extractedContext = directData.context || {};
    const continuumContext = extractedContext.continuumContext || null;

    return {
      success: true,
      command: directData.command,
      parameters: directData.parameters,
      context: extractedContext,
      continuumContext,
      messageType: message.type
    };
  }

  /**
   * Extract from WebSocket execute command message
   */
  private extractFromExecuteCommand(message: DaemonMessage): CommandExtractionResult {
    if (!isExecuteCommandMessage(message)) {
      return {
        success: false,
        error: 'Invalid execute_command message format',
        messageType: message.type
      };
    }

    const execData = message.data;
    if (!execData?.command) {
      return {
        success: false,
        error: 'execute_command message missing command field',
        messageType: message.type
      };
    }

    return {
      success: true,
      command: execData.command,
      parameters: execData.parameters,
      context: execData.context || {},
      continuumContext: null, // WebSocket messages don't typically have continuum context
      messageType: message.type
    };
  }

  /**
   * Extract from HTTP API handling message
   */
  private extractFromHandleApi(message: DaemonMessage): CommandExtractionResult {
    if (!isHandleApiMessage(message)) {
      return {
        success: false,
        error: 'Invalid handle_api message format',
        messageType: message.type
      };
    }

    // For API messages, we need to determine the command from the path
    const apiData = message.data;
    const command = this.extractCommandFromApiPath(apiData.path);
    
    if (!command) {
      return {
        success: false,
        error: `Cannot extract command from API path: ${apiData.path}`,
        messageType: message.type
      };
    }

    return {
      success: true,
      command,
      parameters: apiData.body || apiData.query || {},
      context: { method: apiData.method, path: apiData.path, headers: apiData.headers },
      continuumContext: null,
      messageType: message.type
    };
  }

  /**
   * Extract from widget API handling message
   */
  private extractFromHandleWidgetApi(message: DaemonMessage): CommandExtractionResult {
    if (!isHandleWidgetApiMessage(message)) {
      return {
        success: false,
        error: 'Invalid handle_widget_api message format',
        messageType: message.type
      };
    }

    const widgetData = message.data;
    
    return {
      success: true,
      command: 'widget', // Widget commands are handled specially
      parameters: {
        widgetId: widgetData.widgetId,
        action: widgetData.action,
        parameters: widgetData.parameters
      },
      context: widgetData.context || {},
      continuumContext: null,
      messageType: message.type
    };
  }

  /**
   * Extract command name from API path
   */
  private extractCommandFromApiPath(path: string): string | null {
    // Extract command from paths like /api/commands/screenshot
    const match = path.match(/\/api\/commands\/([^/?]+)/);
    return match ? match[1] : null;
  }

  /**
   * Determine routing strategy based on message and extraction
   */
  private determineRouting(message: DaemonMessage, extraction: CommandExtractionResult): RoutingDecision {
    const messageType = extraction.messageType || message.type;
    
    switch (messageType) {
      case 'handle_api':
        return {
          targetDaemon: 'http-api-handler',
          transformedMessage: message,
          requiresContext: false
        };
        
      case 'execute_command':
        return {
          targetDaemon: 'websocket-handler', 
          transformedMessage: message,
          requiresContext: false
        };
        
      case 'handle_widget_api':
        return {
          targetDaemon: 'http-api-handler', // Widgets use HTTP API handler
          transformedMessage: message,
          requiresContext: false
        };
        
      default: // command.execute and others
        return {
          targetDaemon: 'command-executor',
          transformedMessage: this.createExecutionMessage(extraction),
          requiresContext: Boolean(extraction.continuumContext)
        };
    }
  }

  /**
   * Create standardized execution message
   */
  private createExecutionMessage(extraction: CommandExtractionResult): DaemonMessage {
    return {
      id: `exec-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      type: 'command.execute',
      from: this.name,
      to: 'command-executor',
      data: {
        command: extraction.command!,
        parameters: extraction.parameters,
        context: extraction.context,
        continuumContext: extraction.continuumContext
      } as TypedCommandRequest,
      timestamp: new Date()
    };
  }

  /**
   * Execute with session context
   */
  private async executeWithContext(routing: RoutingDecision, extraction: CommandExtractionResult): Promise<DaemonResponse> {
    this.log(`🔄 ROUTER: Executing with context for ${extraction.command}`);
    
    try {
      const { SessionContext } = await import('../../../context/SessionContext');
      return await SessionContext.withContinuumContext(extraction.continuumContext, async () => {
        return await this.delegateToTargetDaemon(routing);
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return {
        success: false,
        error: `Context execution failed: ${errorMessage}`
      };
    }
  }

  /**
   * Execute directly without session context
   */
  private async executeDirectly(routing: RoutingDecision, extraction: CommandExtractionResult): Promise<DaemonResponse> {
    this.log(`⚡ ROUTER: Executing directly for ${extraction.command}`);
    return await this.delegateToTargetDaemon(routing);
  }

  /**
   * Delegate to target daemon (placeholder - will be implemented with actual daemon communication)
   */
  private async delegateToTargetDaemon(routing: RoutingDecision): Promise<DaemonResponse> {
    this.log(`📤 ROUTER: Delegating to ${routing.targetDaemon}`);
    
    // TODO: Implement actual daemon-to-daemon communication
    // For now, return a success response indicating the routing worked
    return {
      success: true,
      data: {
        routedTo: routing.targetDaemon,
        message: 'Command successfully routed (daemon communication not yet implemented)'
      }
    };
  }
}