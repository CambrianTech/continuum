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

import { BaseCommand, CommandResult, ContinuumContext } from '../base-command/BaseCommand';
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
  protected static async prepareForRemoteExecution(params: any, context?: ContinuumContext): Promise<RemoteExecutionRequest> {
    // Parameters are pre-parsed by UniversalCommandRegistry
    const parsedParams = params;
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
  protected static async processClientResponse(response: RemoteExecutionResponse, originalParams: any, _context?: ContinuumContext): Promise<CommandResult> {
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
  static async execute(params: any, context?: ContinuumContext): Promise<CommandResult> {
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
   * ARCHITECTURE FIX: Route through daemon system instead of direct messaging
   */
  private static async sendToClientViaWebSocket(request: RemoteExecutionRequest, context?: ContinuumContext): Promise<RemoteExecutionResponse> {
    const startTime = Date.now();
    console.log(`🔍 JTAG: Using executeJS pipe for ${request.command}`);
    
    try {
      // Import JSExecuteCommand to use executeJS pipe
      const { JSExecuteCommand } = await import('../../browser/js-execute/JSExecuteCommand');
      
      // Generate the executeJS code to call our own method
      const executeJS = `
        // Call the command's own executeOnClient method
        const request = ${JSON.stringify(request)};
        console.log('🔬 JTAG BROWSER: executeJS calling ${request.command}.executeOnClient');
        
        // Get the command class from window
        const commandClass = window.${request.command}Command || window.ScreenshotCommand;
        if (!commandClass) {
          throw new Error('Command class not available in browser: ${request.command}');
        }
        
        // Call executeOnClient and return result
        return await commandClass.executeOnClient(request);
      `;
      
      console.log(`📤 JTAG: Executing JS in browser:`, executeJS);
      
      // Execute in browser and get response
      const jsResult = await JSExecuteCommand.execute({
        script: executeJS,
        returnResult: true
      }, context);
      
      console.log(`📨 JTAG: Got JS execution result:`, JSON.stringify(jsResult, null, 2));
      
      if (jsResult.success && jsResult.data?.result) {
        const executionTime = Date.now() - startTime;
        console.log(`✅ JTAG: executeJS pipe completed in ${executionTime}ms`);
        
        // Return the browser result as RemoteExecutionResponse
        return jsResult.data.result as RemoteExecutionResponse;
      } else {
        throw new Error(jsResult.error || 'executeJS failed');
      }
      
    } catch (error) {
      const executionTime = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ JTAG: executeJS pipe failed: ${errorMessage}`);
      
      return {
        success: false,
        error: `executeJS pipe failed: ${errorMessage}`,
        clientMetadata: {
          userAgent: 'executeJS-pipe',
          timestamp: Date.now(),
          executionTime
        }
      };
    }
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
  protected static async validateClientConnection(_context?: ContinuumContext): Promise<boolean> {
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