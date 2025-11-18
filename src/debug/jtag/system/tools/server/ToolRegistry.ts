/**
 * ToolRegistry - Dynamic tool discovery from Commands
 *
 * Replaces hardcoded tool lists with dynamic command discovery.
 * Tools are auto-generated from CommandSignature metadata via 'list' command.
 *
 * Architecture:
 * 1. Query 'list' command on startup → get all CommandSignatures
 * 2. Auto-generate tool handlers for ANY command (universal wrapper)
 * 3. Auto-generate tool descriptions for AI system prompts
 *
 * Benefits:
 * - New commands (decision/propose, decision/rank, etc.) automatically become tools
 * - Zero hardcoding - single source of truth is CommandRegistry
 * - Consistent tool behavior across all commands
 */

import { Commands } from '../../core/shared/Commands';
import type { CommandSignature } from '../../../commands/list/shared/ListTypes';
import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Tool metadata for AI consumption
 */
export interface ToolDefinition {
  name: string;
  description: string;
  category: string;
  parameters: Record<string, {
    type: string;
    required: boolean;
    description?: string;
  }>;
  examples?: string[];
}

/**
 * Tool execution result
 */
export interface ToolExecutionResult {
  toolName: string;
  success: boolean;
  content?: string;
  error?: string;
}

/**
 * ToolRegistry - Dynamic command discovery and tool generation
 */
export class ToolRegistry {
  private static instance: ToolRegistry;
  private tools: Map<string, ToolDefinition> = new Map();
  private initialized = false;

  private constructor() {}

  /**
   * Get singleton instance
   */
  static getInstance(): ToolRegistry {
    if (!ToolRegistry.instance) {
      ToolRegistry.instance = new ToolRegistry();
    }
    return ToolRegistry.instance;
  }

  /**
   * Initialize tool registry by querying 'list' command
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      console.log('⚙️ ToolRegistry: Already initialized');
      return;
    }

    console.log('⚙️ ToolRegistry: Discovering available commands...');

    try {
      const result = await Commands.execute('list', {}) as unknown as {
        commands?: CommandSignature[];
        success: boolean;
        error?: string;
      };

      if (!result.success || !result.commands) {
        throw new Error(`Failed to list commands: ${result.error}`);
      }

      // Convert CommandSignatures to ToolDefinitions
      for (const cmd of result.commands) {
        const toolDef: ToolDefinition = {
          name: cmd.name,
          description: cmd.description,
          category: cmd.category,
          parameters: cmd.params
        };
        this.tools.set(cmd.name, toolDef);
      }

      this.initialized = true;
      console.log(`✅ ToolRegistry: Discovered ${this.tools.size} commands as tools`);
    } catch (error) {
      console.error(`❌ ToolRegistry: Initialization failed:`, error);
      throw error;
    }
  }

  /**
   * Get all available tools
   */
  getAllTools(): ToolDefinition[] {
    if (!this.initialized) {
      throw new Error('ToolRegistry not initialized - call initialize() first');
    }
    return Array.from(this.tools.values());
  }

  /**
   * Get specific tool by name
   */
  getTool(name: string): ToolDefinition | undefined {
    return this.tools.get(name);
  }

  /**
   * Check if tool exists
   */
  hasTool(name: string): boolean {
    return this.tools.has(name);
  }

  /**
   * Execute a tool (universal command wrapper)
   *
   * This is the "adapter" the user mentioned - ONE function that can execute ANY command
   */
  async executeTool(
    toolName: string,
    parameters: Record<string, string>,
    contextId: UUID
  ): Promise<ToolExecutionResult> {
    if (!this.hasTool(toolName)) {
      return {
        toolName,
        success: false,
        error: `Unknown tool: ${toolName}`
      };
    }

    // NO try-catch - let exceptions bubble to PersonaResponseGenerator
    // Commands.execute (via CommandDaemon) returns {success: false, error} for expected command failures
    // Only UNEXPECTED exceptions (system crashes) should bubble up as exceptions

    // Parse JSON parameters if needed
    const parsedParams: Record<string, any> = {};
    const toolDef = this.getTool(toolName)!;

    for (const [key, value] of Object.entries(parameters)) {
      const paramDef = toolDef.parameters[key];

      // Try to parse JSON for complex types
      if (paramDef && (paramDef.type === 'object' || paramDef.type === 'array')) {
        try {
          parsedParams[key] = JSON.parse(value);
        } catch {
          parsedParams[key] = value; // Fallback to string
        }
      } else if (paramDef && paramDef.type === 'number') {
        parsedParams[key] = parseInt(value, 10);
      } else if (paramDef && paramDef.type === 'boolean') {
        parsedParams[key] = value === 'true';
      } else {
        parsedParams[key] = value;
      }
    }

    // Execute command via Commands.execute
    const result: any = await Commands.execute(toolName as any, parsedParams as any);

    if (!result.success) {
      return {
        toolName,
        success: false,
        error: result.error || 'Command execution failed'
      };
    }

    // Format result based on command type
    const content = this.formatToolResult(toolName, result);

    return {
      toolName,
      success: true,
      content
    };
  }

  /**
   * Format tool execution result for AI consumption
   */
  private formatToolResult(toolName: string, result: any): string {
    // Handle specific command patterns
    if (toolName === 'list' && result.commands) {
      return result.commands
        .map((cmd: any) => `${cmd.name} - ${cmd.description} [${cmd.category}]`)
        .join('\n');
    }

    if (toolName.startsWith('data/list') && result.items) {
      return `Collection: ${result.collection || 'unknown'}\nCount: ${result.count || result.items.length}\n\nResults:\n${JSON.stringify(result.items, null, 2)}`;
    }

    if (toolName.startsWith('data/read') && result.data) {
      return `Collection: ${result.collection || 'unknown'}\nID: ${result.id || 'unknown'}\n\nData:\n${JSON.stringify(result.data, null, 2)}`;
    }

    if (toolName === 'code/read' && result.content) {
      const lineRange = result.startLine && result.endLine
        ? ` (lines ${result.startLine}-${result.endLine})`
        : '';
      return `Path: ${result.path || 'unknown'}${lineRange}\n\n${result.content}`;
    }

    if (toolName === 'chat/export' && result.markdown) {
      return `Exported ${result.messageCount || 0} messages:\n\n${result.markdown}`;
    }

    // Generic fallback - JSON stringify the result
    return JSON.stringify(result, null, 2);
  }

  /**
   * Generate tool descriptions for AI system prompt
   *
   * This replaces the hardcoded tool list in ChatRAGBuilder
   */
  generateToolDocumentation(): string {
    if (!this.initialized) {
      throw new Error('ToolRegistry not initialized - call initialize() first');
    }

    const tools = this.getAllTools();

    if (tools.length === 0) {
      return 'No tools available.';
    }

    const toolDescriptions = tools.map((tool, index) => {
      const requiredParams = Object.entries(tool.parameters)
        .filter(([_, def]) => def.required)
        .map(([name, def]) => `<${name}>${def.description || name}</${name}>`)
        .join(' ');

      const optionalParams = Object.entries(tool.parameters)
        .filter(([_, def]) => !def.required)
        .map(([name, def]) => `<${name}>${def.description || name}</${name}>`)
        .join(' ');

      let desc = `${index + 1}. ${tool.name} - ${tool.description}`;

      if (requiredParams) {
        desc += `\n   Required: ${requiredParams}`;
      }

      if (optionalParams) {
        desc += `\n   Optional: ${optionalParams}`;
      }

      return desc;
    }).join('\n\n');

    return `AVAILABLE TOOLS:
You have access to tools for reading code, querying data, and system operations. To use a tool, include a tool invocation in your response using this exact XML format:

<tool_use>
<tool_name>command/name</tool_name>
<parameters>
<paramName>value</paramName>
</parameters>
</tool_use>

Available tools:
${toolDescriptions}

Tool execution flow:
1. Include <tool_use> blocks in your response
2. System executes tools and provides results
3. You receive results and provide final analysis

NOTE: Tool calls are removed from visible response. Only your text is shown to users.`;
  }
}
