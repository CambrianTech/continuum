/**
 * RemoteCommand - Universal command orchestration across execution environments
 * 
 * Handles distributed command execution across:
 * 1. Browser clients (WebSocket to browser DOM/APIs)
 * 2. Python processes (HTTP/WebSocket to Python Continuum API)  
 * 3. Remote Continuum instances (WebSocket between installations)
 * 4. AI persona environments (distributed AI collaboration)
 * 5. Hybrid multi-environment workflows
 * 
 * Provides unified interface for:
 * - Request marshaling and environment routing
 * - Cross-environment execution coordination  
 * - Response aggregation and error handling
 * - Timeout and failure recovery across network boundaries
 */

import { BaseCommand, CommandResult, CommandContext } from '../base-command/BaseCommand';
import {
  RemoteCommandType
} from '../../../types/shared/CommandOperationTypes';

export interface RemoteExecutionRequest {
  command: RemoteCommandType;
  params: any;
  sessionId?: string | undefined;
  timeout?: number;
}

export interface RemoteExecutionResponse {
  success: boolean;
  data?: any;
  error?: string;
  clientMetadata?: {
    userAgent: string;
    timestamp: number;
    executionTime: number;
  };
}

export abstract class RemoteCommand extends BaseCommand {
  /**
   * Server-side preparation before sending to client
   */
  protected static async prepareForRemoteExecution(params: any, context?: CommandContext): Promise<RemoteExecutionRequest> {
    const parsedParams = this.parseParams(params);
    
    return {
      command: this.getDefinition().name as RemoteCommandType,
      params: parsedParams,
      sessionId: context?.sessionId || undefined,
      timeout: this.getRemoteTimeout()
    };
  }

  /**
   * Execute on the client side (implemented by subclasses)
   */
  protected static async executeOnClient(_request: RemoteExecutionRequest): Promise<RemoteExecutionResponse> {
    throw new Error('executeOnClient() must be implemented by subclass');
  }

  /**
   * Process client response on server side
   */
  protected static async processClientResponse(response: RemoteExecutionResponse, originalParams: any, _context?: CommandContext): Promise<CommandResult> {
    if (!response.success) {
      return this.createErrorResult(`Client execution failed: ${response.error}`);
    }

    return this.createSuccessResult(
      'Remote command executed successfully',
      {
        result: response.data,
        client: response.clientMetadata,
        originalParams
      }
    );
  }

  /**
   * Standard execute implementation with WebSocket coordination
   */
  static async execute(params: any, context?: CommandContext): Promise<CommandResult> {
    const startTime = Date.now();
    console.log(`🚀 JTAG: Starting RemoteCommand execution - command: ${this.getDefinition().name}`);
    console.log(`📋 JTAG: Parameters received:`, JSON.stringify(params, null, 2));
    
    try {
      // 1. Prepare request for remote execution
      console.log(`📝 JTAG: Preparing request for remote execution`);
      const request = await this.prepareForRemoteExecution(params, context);
      console.log(`✅ JTAG: Request prepared - command: ${request.command}, sessionId: ${request.sessionId}, timeout: ${request.timeout}ms`);
      
      // 2. Send to client via WebSocket and wait for response
      console.log(`📡 JTAG: Sending request to client via WebSocket`);
      const response = await this.sendToClientViaWebSocket(request, context);
      console.log(`📨 JTAG: Received response from client - success: ${response.success}`);
      
      // 3. Process the client response with context
      console.log(`⚙️ JTAG: Processing client response`);
      const result = await this.processClientResponse(response, params, context);
      
      const executionTime = Date.now() - startTime;
      console.log(`✅ JTAG: RemoteCommand execution completed in ${executionTime}ms - success: ${result.success}`);
      
      return result;
      
    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ JTAG: RemoteCommand execution failed after ${executionTime}ms: ${errorMessage}`);
      return this.createErrorResult(`Remote command failed: ${errorMessage}`);
    }
  }

  /**
   * WebSocket communication infrastructure
   */
  private static async sendToClientViaWebSocket(request: RemoteExecutionRequest, context?: CommandContext): Promise<RemoteExecutionResponse> {
    const startTime = Date.now();
    console.log(`🔍 JTAG: Initiating WebSocket communication for ${request.command}`);
    
    // Generate correlation ID for request/response matching
    const correlationId = `remote_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
    console.log(`🔑 JTAG: Generated correlation ID: ${correlationId}`);
    
    const sessionId = request.sessionId || context?.sessionId;
    if (!sessionId) {
      const error = 'No session ID available for WebSocket communication';
      console.error(`❌ JTAG: ${error}`);
      return {
        success: false,
        error: `WebSocket communication failed: ${error}`,
        clientMetadata: {
          userAgent: 'Unknown',
          timestamp: Date.now(),
          executionTime: 0
        }
      };
    }
    
    console.log(`🔍 JTAG: Session ID: ${sessionId}`);
    
    // For now, return a proper error since real WebSocket integration needs more infrastructure
    const executionTime = Date.now() - startTime;
    const errorMessage = 'Real WebSocket communication not yet implemented - needs browser message handler';
    console.error(`❌ JTAG: ${errorMessage} after ${executionTime}ms`);
    
    return {
      success: false,
      error: `WebSocket communication failed: ${errorMessage}`,
      clientMetadata: {
        userAgent: 'Unknown',
        timestamp: Date.now(),
        executionTime: 0
      }
    };
  }


  /**
   * Get timeout for remote execution (subclasses can override)
   */
  protected static getRemoteTimeout(): number {
    return 30000; // 30 seconds default
  }

  /**
   * Validate that WebSocket connection exists for session
   */
  protected static async validateClientConnection(_context?: CommandContext): Promise<boolean> {
    // TODO: Check if WebSocket connection exists for context.sessionId
    // Return false if no connection, true if connected
    return true; // Mock implementation
  }


  /**
   * Helper to create client-side execution script
   */
  protected static createClientScript(functionBody: string): string {
    return `
      (async function() {
        try {
          ${functionBody}
        } catch (error) {
          return {
            success: false,
            error: error.message,
            stack: error.stack
          };
        }
      })();
    `;
  }
}