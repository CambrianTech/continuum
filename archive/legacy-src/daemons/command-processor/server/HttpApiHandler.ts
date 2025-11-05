/**
 * HTTP API Handler - Focused daemon for HTTP API request handling
 * 
 * Extracted from CommandProcessorDaemon as part of symmetric architecture migration.
 * Handles HTTP API requests, validates parameters, and coordinates with command execution.
 * 
 * Responsibilities:
 * - Process HTTP API requests and routing
 * - Validate API request formats and parameters
 * - Transform HTTP requests into standardized command messages
 * - Handle HTTP-specific response formatting
 */

import { BaseDaemon } from '../../base/BaseDaemon';
import { DaemonMessage, DaemonResponse } from '../../base/DaemonProtocol';
import { DaemonType } from '../../base/DaemonTypes';
import { 
  isHandleApiMessage,
  isHandleWidgetApiMessage
} from '../shared';

// ✅ HTTP API REQUEST CONTEXT
export interface HttpApiContext {
  readonly method: string;
  readonly path: string;
  readonly headers: Record<string, string>;
  readonly query?: Record<string, any>;
  readonly body?: any;
  readonly timestamp: Date;
}

// ✅ API VALIDATION RESULT
export interface ApiValidationResult {
  readonly success: boolean;
  readonly command?: string;
  readonly parameters?: unknown;
  readonly context?: HttpApiContext;
  readonly error?: string;
  readonly statusCode?: number;
}

// ✅ HTTP RESPONSE FORMAT
export interface HttpApiResponse {
  readonly statusCode: number;
  readonly headers: Record<string, string>;
  readonly body: any;
  readonly contentType: string;
}

export class HttpApiHandler extends BaseDaemon {
  public readonly name = 'http-api-handler';
  public readonly version = '1.0.0';
  public readonly id: string;
  public readonly daemonType = DaemonType.COMMAND_PROCESSOR; // Will create new type later
  public readonly config = {
    name: this.name,
    version: this.version,
    port: 9004, // Different port from other daemons
    autoStart: true,
    dependencies: ['command-router'],
    healthCheck: { interval: 30000, timeout: 5000, retries: 3 },
    resources: { maxMemory: 256, maxCpu: 40 }
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
      'handle_api',         // Main HTTP API handling
      'handle_widget_api',  // Widget-specific API handling
      'api.validate',       // API request validation
      'api.transform'       // Transform API request to command
    ];
  }

  protected async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    this.log(`🌐 HTTP API: Handling message type: ${message.type}`);
    
    try {
      switch (message.type) {
        case 'handle_api':
          return await this.handleApiRequest(message);
          
        case 'handle_widget_api':
          return await this.handleWidgetApiRequest(message);
          
        case 'api.validate':
          return await this.handleApiValidation(message);
          
        case 'api.transform':
          return await this.handleApiTransformation(message);
          
        default:
          return {
            success: false,
            error: `Unsupported message type: ${message.type}`
          };
      }
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      this.log(`❌ HTTP API: Error handling message: ${errorMessage}`);
      
      return {
        success: false,
        error: `HTTP API handling failed: ${errorMessage}`
      };
    }
  }

  /**
   * Handle main HTTP API request
   */
  private async handleApiRequest(message: DaemonMessage): Promise<DaemonResponse> {
    if (!isHandleApiMessage(message)) {
      return this.createErrorResponse('Invalid handle_api message format', 400);
    }

    const apiData = message.data;
    this.log(`🔍 HTTP API: Processing ${apiData.method} ${apiData.path}`);

    // Validate API request
    const validation = this.validateApiRequest(apiData);
    if (!validation.success) {
      return this.createErrorResponse(validation.error!, validation.statusCode || 400);
    }

    // Transform to command execution format
    const commandMessage = this.transformApiToCommand(validation);
    
    // Route to command executor through router
    return await this.routeToCommandExecution(commandMessage, validation.context!);
  }

  /**
   * Handle widget-specific API request
   */
  private async handleWidgetApiRequest(message: DaemonMessage): Promise<DaemonResponse> {
    if (!isHandleWidgetApiMessage(message)) {
      return this.createErrorResponse('Invalid handle_widget_api message format', 400);
    }

    const widgetData = message.data;
    this.log(`🎨 WIDGET API: Processing widget ${widgetData.widgetId} action ${widgetData.action}`);

    // Validate widget request
    const validation = this.validateWidgetRequest(widgetData);
    if (!validation.success) {
      return this.createErrorResponse(validation.error!, validation.statusCode || 400);
    }

    // Transform widget request to command
    const commandMessage = this.transformWidgetToCommand(validation);
    
    // Route to command execution
    return await this.routeToCommandExecution(commandMessage, validation.context!);
  }

  /**
   * Handle API validation request
   */
  private async handleApiValidation(message: DaemonMessage): Promise<DaemonResponse> {
    const apiData = message.data;
    const validation = this.validateApiRequest(apiData);
    
    return {
      success: validation.success,
      data: validation,
      ...(validation.error && { error: validation.error })
    };
  }

  /**
   * Handle API transformation request
   */
  private async handleApiTransformation(message: DaemonMessage): Promise<DaemonResponse> {
    const apiData = message.data;
    const validation = this.validateApiRequest(apiData);
    
    if (!validation.success) {
      return this.createErrorResponse(validation.error!, validation.statusCode || 400);
    }

    const commandMessage = this.transformApiToCommand(validation);
    
    return {
      success: true,
      data: { commandMessage, context: validation.context }
    };
  }

  /**
   * Validate API request format and extract command information
   */
  private validateApiRequest(apiData: any): ApiValidationResult {
    if (!apiData || typeof apiData !== 'object') {
      return {
        success: false,
        error: 'API data is required and must be an object',
        statusCode: 400
      };
    }

    const { method, path, headers, query, body } = apiData;

    if (!method || !path) {
      return {
        success: false,
        error: 'API request must include method and path',
        statusCode: 400
      };
    }

    // Extract command from API path
    const command = this.extractCommandFromPath(path);
    if (!command) {
      return {
        success: false,
        error: `Cannot extract command from path: ${path}`,
        statusCode: 404
      };
    }

    // Validate HTTP method for command
    if (!this.isValidMethodForCommand(method, command)) {
      return {
        success: false,
        error: `Method ${method} not allowed for command ${command}`,
        statusCode: 405
      };
    }

    const context: HttpApiContext = {
      method,
      path,
      headers: headers || {},
      query,
      body,
      timestamp: new Date()
    };

    // Determine parameters from query or body based on method
    const parameters = method === 'GET' ? query : body;

    return {
      success: true,
      command,
      parameters,
      context
    };
  }

  /**
   * Validate widget API request
   */
  private validateWidgetRequest(widgetData: any): ApiValidationResult {
    if (!widgetData || typeof widgetData !== 'object') {
      return {
        success: false,
        error: 'Widget data is required and must be an object',
        statusCode: 400
      };
    }

    const { widgetId, action, parameters } = widgetData;

    if (!widgetId) {
      return {
        success: false,
        error: 'Widget ID is required',
        statusCode: 400
      };
    }

    if (!action) {
      return {
        success: false,
        error: 'Widget action is required',
        statusCode: 400
      };
    }

    const context: HttpApiContext = {
      method: 'POST',
      path: `/api/widgets/${widgetId}/${action}`,
      headers: { 'content-type': 'application/json' },
      body: parameters,
      timestamp: new Date()
    };

    return {
      success: true,
      command: 'widget', // Widget system handles action routing internally via dynamic discovery
      parameters: {
        widgetId,
        action,
        parameters: parameters || {}
      },
      context
    };
  }

  /**
   * Extract command from API path
   */
  private extractCommandFromPath(path: string): string | null {
    // Handle different API path patterns:
    // /api/commands/screenshot
    // /api/widget/... (handled separately)
    // /screenshot (direct command paths)
    
    const patterns = [
      /^\/api\/commands\/([^/?]+)/,  // /api/commands/command-name
      /^\/([^/?]+)/                   // /command-name
    ];

    for (const pattern of patterns) {
      const match = path.match(pattern);
      if (match) {
        return match[1];
      }
    }

    return null;
  }

  /**
   * Check if HTTP method is valid for command
   */
  private isValidMethodForCommand(method: string, command: string): boolean {
    const methodUpper = method.toUpperCase();
    
    // Read-only commands should use GET
    const readOnlyCommands = ['help', 'status', 'info', 'screenshot'];
    if (readOnlyCommands.includes(command)) {
      return ['GET', 'POST'].includes(methodUpper); // Allow both for flexibility
    }
    
    // Write commands should use POST/PUT
    const writeCommands = ['file', 'exec', 'widget'];
    if (writeCommands.includes(command)) {
      return ['POST', 'PUT'].includes(methodUpper);
    }
    
    // Default: allow GET and POST
    return ['GET', 'POST'].includes(methodUpper);
  }

  /**
   * Transform API request to command message
   */
  private transformApiToCommand(validation: ApiValidationResult): DaemonMessage {
    return {
      id: `api-cmd-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      type: 'command.route',
      from: this.name,
      to: 'command-router',
      data: {
        command: validation.command!,
        parameters: validation.parameters,
        context: {
          source: 'http-api',
          httpContext: validation.context
        }
      },
      timestamp: new Date()
    };
  }

  /**
   * Transform widget request to command message
   */
  private transformWidgetToCommand(validation: ApiValidationResult): DaemonMessage {
    return {
      id: `widget-cmd-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
      type: 'command.route',
      from: this.name,
      to: 'command-router',
      data: {
        command: validation.command!,
        parameters: validation.parameters,
        context: {
          source: 'widget-api',
          httpContext: validation.context
        }
      },
      timestamp: new Date()
    };
  }

  /**
   * Route command to execution through command router
   */
  private async routeToCommandExecution(commandMessage: DaemonMessage, httpContext: HttpApiContext): Promise<DaemonResponse> {
    this.log(`📤 HTTP API: Routing command ${(commandMessage.data as any).command} to execution`);
    
    try {
      // TODO: Implement actual daemon-to-daemon communication
      // For now, return formatted HTTP response indicating routing worked
      
      const httpResponse = this.formatHttpResponse({
        success: true,
        data: {
          message: 'Command routed for execution',
          command: (commandMessage.data as any).command,
          routed: true
        }
      }, httpContext);
      
      return {
        success: true,
        data: httpResponse
      };
      
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      
      const httpResponse = this.formatHttpResponse({
        success: false,
        error: errorMessage
      }, httpContext, 500);
      
      return {
        success: false,
        data: httpResponse,
        error: errorMessage
      };
    }
  }

  /**
   * Format response as HTTP-compatible format
   */
  private formatHttpResponse(
    result: { success: boolean; data?: any; error?: string }, 
    context: HttpApiContext,
    statusCode?: number
  ): HttpApiResponse {
    const status = statusCode || (result.success ? 200 : 400);
    
    return {
      statusCode: status,
      headers: {
        'content-type': 'application/json',
        'x-continuum-handler': this.name,
        'x-continuum-timestamp': context.timestamp.toISOString()
      },
      body: result,
      contentType: 'application/json'
    };
  }

  /**
   * Create error response with HTTP status code
   */
  private createErrorResponse(error: string, statusCode: number = 400): DaemonResponse {
    return {
      success: false,
      error,
      data: {
        statusCode,
        headers: { 'content-type': 'application/json' },
        body: { success: false, error },
        contentType: 'application/json'
      }
    };
  }

  /**
   * Get API handling statistics
   */
  public getApiStats(): {
    totalRequests: number;
    successRate: number;
    commonCommands: Record<string, number>;
    avgResponseTime: number;
  } {
    // TODO: Implement actual statistics tracking
    return {
      totalRequests: 0,
      successRate: 0,
      commonCommands: {},
      avgResponseTime: 0
    };
  }
}