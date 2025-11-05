/**
 * MCP (Model Context Protocol) Integration Parser
 * Enables MCP servers and tools to communicate with Continuum
 */

import { IntegrationParser } from './IntegrationParser';

interface MCPRequest {
  jsonrpc: '2.0';
  method: string;
  params?: any;
  id?: string | number;
}

interface MCPToolCall {
  name: string;
  arguments?: Record<string, any>;
}

export class MCPIntegrationParser implements IntegrationParser {
  name = 'MCP';
  priority = 85; // High priority for MCP ecosystem

  canHandle(params: unknown): boolean {
    return (
      params !== null &&
      typeof params === 'object' &&
      'jsonrpc' in params &&
      'method' in params &&
      (params as any).jsonrpc === '2.0'
    );
  }

  parse<T>(params: unknown): T {
    const mcpRequest = params as MCPRequest;
    
    // Map MCP methods to Continuum commands
    const result = this.translateMCPMethod(mcpRequest);
    
    // Preserve MCP context for debugging/tracing
    result._mcpContext = {
      method: mcpRequest.method,
      id: mcpRequest.id,
      jsonrpc: mcpRequest.jsonrpc
    };
    
    return result as T;
  }

  private translateMCPMethod(request: MCPRequest): any {
    const { method, params } = request;
    
    switch (method) {
      case 'tools/list':
        return { command: 'help', format: 'tools' };
        
      case 'tools/call':
        const toolCall = params as MCPToolCall;
        return {
          command: toolCall.name,
          ...toolCall.arguments
        };
        
      case 'resources/list':
        return { command: 'projects', format: 'resources' };
        
      case 'prompts/list':
        return { command: 'personas', format: 'prompts' };
        
      case 'prompts/get':
        return { 
          command: 'persona', 
          action: 'get',
          name: params?.name 
        };
        
      case 'resources/read':
        return {
          command: 'data-marshal',
          operation: 'read',
          path: params?.uri
        };
        
      case 'completion/complete':
        return {
          command: 'chat',
          message: params?.prompt,
          context: params?.argument?.context
        };
        
      default:
        // For unknown methods, try to extract command from method name
        const commandMatch = method.match(/(\w+)\/(\w+)/);
        if (commandMatch) {
          return {
            command: commandMatch[2],
            service: commandMatch[1],
            ...params
          };
        }
        
        // Fallback: pass through with method as command
        return {
          command: method,
          ...params
        };
    }
  }
}