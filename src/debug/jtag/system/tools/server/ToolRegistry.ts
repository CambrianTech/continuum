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
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { CommandSignature } from '../../../commands/list/shared/ListTypes';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { MediaItem } from '../../data/entities/ChatMessageEntity';
import type { CommandParams, CommandResult } from '../../core/types/JTAGTypes';

/**
 * Type guard for command results that include a success field
 * Many commands add success: boolean to their result type
 */
interface ResultWithSuccess {
  success: boolean;
  error?: string;
}

function hasSuccessField(result: CommandResult): result is CommandResult & ResultWithSuccess {
  return 'success' in result && typeof (result as { success?: unknown }).success === 'boolean';
}

/**
 * Type guard for command results that include media
 * Some commands (screenshot, file/read, etc.) include media in their results
 */
interface ResultWithMedia {
  media?: MediaItem | MediaItem[];
}

function hasMediaField(result: CommandResult): result is CommandResult & ResultWithMedia {
  return 'media' in result;
}

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
 * Tool execution result with structured media support
 */
export interface ToolExecutionResult {
  toolName: string;
  success: boolean;
  content?: string;
  media?: MediaItem[];  // Structured media from command results
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
          category: 'command',  // All commands are category 'command'
          parameters: cmd.params || {}
        };
        this.tools.set(cmd.name, toolDef);
      }

      // Register built-in meta-tools for tool discovery
      this.registerBuiltInTools();

      this.initialized = true;
      console.log(`✅ ToolRegistry: Discovered ${this.tools.size} commands as tools (including ${ToolRegistry.BUILT_IN_TOOLS.length} meta-tools)`);
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

  // ===========================================================================
  // TOOL DISCOVERY - For personas to find tools without loading all into context
  // ===========================================================================

  /**
   * Search tools by keyword (matches name and description)
   * Same algorithm as MCP search_tools for consistency
   */
  searchTools(query: string, category?: string, limit: number = 10): Array<{ name: string; description: string; category: string }> {
    const queryLower = query.toLowerCase();
    const results: Array<{ name: string; description: string; category: string; score: number }> = [];

    for (const tool of this.tools.values()) {
      const nameLower = tool.name.toLowerCase();
      const descLower = (tool.description || '').toLowerCase();

      // Category filter
      if (category) {
        const categoryPrefix = category.endsWith('/') ? category : `${category}/`;
        if (!nameLower.startsWith(categoryPrefix) && nameLower !== category) {
          continue;
        }
      }

      // Score matches
      let score = 0;
      if (nameLower.includes(queryLower)) score += 10;
      if (nameLower.startsWith(queryLower)) score += 5;
      if (descLower.includes(queryLower)) score += 3;

      // Exact segment match (e.g., "css" matches "widget-css")
      const segments = nameLower.split(/[\/\-_]/);
      if (segments.includes(queryLower)) score += 8;

      if (score > 0) {
        const toolCategory = nameLower.includes('/') ? nameLower.split('/')[0] : 'root';
        results.push({
          name: tool.name,
          description: tool.description || tool.name,
          category: toolCategory,
          score,
        });
      }
    }

    // Sort by score descending, then name
    results.sort((a, b) => b.score - a.score || a.name.localeCompare(b.name));
    return results.slice(0, limit).map(({ name, description, category }) => ({ name, description, category }));
  }

  /**
   * List tools by category (for browsing all available tools)
   */
  listToolsByCategory(category?: string, limit: number = 50): Array<{ name: string; description: string; category: string }> {
    const results: Array<{ name: string; description: string; category: string }> = [];

    for (const tool of this.tools.values()) {
      const nameLower = tool.name.toLowerCase();
      const toolCategory = nameLower.includes('/') ? nameLower.split('/')[0] : 'root';

      // Category filter
      if (category) {
        const categoryPrefix = category.endsWith('/') ? category : `${category}/`;
        if (!nameLower.startsWith(categoryPrefix) && toolCategory !== category) {
          continue;
        }
      }

      results.push({
        name: tool.name,
        description: tool.description || tool.name,
        category: toolCategory,
      });
    }

    // Sort by category then name
    results.sort((a, b) => a.category.localeCompare(b.category) || a.name.localeCompare(b.name));
    return results.slice(0, limit);
  }

  /**
   * Get available categories
   */
  getCategories(): string[] {
    const categories = new Set<string>();
    for (const tool of this.tools.values()) {
      const name = tool.name.toLowerCase();
      const category = name.includes('/') ? name.split('/')[0] : 'root';
      categories.add(category);
    }
    return Array.from(categories).sort();
  }

  // ===========================================================================
  // BUILT-IN META-TOOLS - Tools for discovering other tools
  // ===========================================================================

  private static readonly BUILT_IN_TOOLS: ToolDefinition[] = [
    {
      name: 'search_tools',
      description: 'Search for tools by keyword. Use this to find specialized tools without knowing exact names.',
      category: 'meta',
      parameters: {
        query: { type: 'string', required: true, description: 'Search keyword (e.g., "screenshot", "css", "chat")' },
        category: { type: 'string', required: false, description: 'Optional category filter (e.g., "interface", "ai", "data")' },
        limit: { type: 'number', required: false, description: 'Max results (default: 10)' },
      },
    },
    {
      name: 'list_tools',
      description: 'List available tools, optionally filtered by category. Use this to browse all capabilities.',
      category: 'meta',
      parameters: {
        category: { type: 'string', required: false, description: 'Filter by category (e.g., "interface", "collaboration", "ai")' },
        limit: { type: 'number', required: false, description: 'Max results (default: 50)' },
      },
    },
    {
      name: 'list_categories',
      description: 'List all available tool categories.',
      category: 'meta',
      parameters: {},
    },
  ];

  /**
   * Register built-in meta-tools
   */
  private registerBuiltInTools(): void {
    for (const tool of ToolRegistry.BUILT_IN_TOOLS) {
      this.tools.set(tool.name, tool);
    }
  }

  /**
   * Check if tool is a built-in meta-tool
   */
  private isBuiltInTool(name: string): boolean {
    return ToolRegistry.BUILT_IN_TOOLS.some(t => t.name === name);
  }

  /**
   * Execute built-in meta-tool
   */
  private executeBuiltInTool(toolName: string, parameters: Record<string, string>): ToolExecutionResult {
    switch (toolName) {
      case 'search_tools': {
        const query = parameters.query || '';
        const category = parameters.category;
        const limit = parameters.limit ? parseInt(parameters.limit, 10) : 10;
        const results = this.searchTools(query, category, limit);
        return {
          toolName,
          success: true,
          content: JSON.stringify({
            query,
            category: category || 'all',
            count: results.length,
            tools: results,
          }, null, 2),
        };
      }
      case 'list_tools': {
        const category = parameters.category;
        const limit = parameters.limit ? parseInt(parameters.limit, 10) : 50;
        const results = this.listToolsByCategory(category, limit);
        return {
          toolName,
          success: true,
          content: JSON.stringify({
            category: category || 'all',
            count: results.length,
            tools: results,
          }, null, 2),
        };
      }
      case 'list_categories': {
        const categories = this.getCategories();
        return {
          toolName,
          success: true,
          content: JSON.stringify({
            count: categories.length,
            categories,
          }, null, 2),
        };
      }
      default:
        return {
          toolName,
          success: false,
          error: `Unknown built-in tool: ${toolName}`,
        };
    }
  }

  /**
   * Execute a tool (universal command wrapper)
   *
   * This is the "adapter" the user mentioned - ONE function that can execute ANY command
   */
  async executeTool(
    toolName: string,
    parameters: Record<string, string>,
    sessionId: UUID,  // SessionId of the tool executor (AI's own session for sandboxing)
    contextId: UUID,
    context?: import('../../core/types/JTAGTypes').JTAGContext  // Optional enriched context (with callerType for caller-adaptive output)
  ): Promise<ToolExecutionResult> {
    // Handle built-in meta-tools first (no command execution needed)
    if (this.isBuiltInTool(toolName)) {
      return this.executeBuiltInTool(toolName, parameters);
    }

    if (!this.hasTool(toolName)) {
      return {
        toolName,
        success: false,
        error: `Unknown tool: ${toolName}. Use search_tools to find available tools.`
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
    // Pass sessionId explicitly to override auto-injected value (AI's own sessionId for sandboxing)
    // Pass context explicitly if provided (PersonaUser's enriched context with callerType)
    // This enables caller-adaptive command output (e.g., PersonaUsers receive media field in screenshot results)
    const commandParams: Record<string, any> = {
      sessionId,  // Pass AI's sessionId for proper attribution
      contextId,  // Some commands may use this (will be ignored if not needed)
      ...parsedParams
    };

    // Include enriched context if provided (enables caller-adaptive output)
    if (context) {
      commandParams.context = context;
    }

    const result = await Commands.execute(toolName, commandParams);

    // Check if command executed successfully (if result has success field)
    // Not all commands have success field, so we check first with type guard
    if (hasSuccessField(result) && !result.success) {
      return {
        toolName,
        success: false,
        error: result.error || 'Command execution failed'
      };
    }

    // Extract media if present (screenshot, file/read, etc.)
    // Not all commands return media, so we check first with type guard
    const media: MediaItem[] | undefined = hasMediaField(result) && result.media
      ? (Array.isArray(result.media) ? result.media : [result.media])
      : undefined;

    // Format result based on command type
    const content = this.formatToolResult(toolName, result);

    return {
      toolName,
      success: true,
      content,
      media  // ← Preserve structured media
    };
  }

  /**
   * Format tool execution result for AI consumption
   */
  private formatToolResult(toolName: string, result: any): string {
    // Handle specific command patterns
    if (toolName === 'list' && result.commands) {
      return result.commands
        .map((cmd: any) => `${cmd.name} - ${cmd.description}`)
        .join('\n');
    }

    if (toolName.startsWith(DATA_COMMANDS.LIST) && result.items) {
      return `Collection: ${result.collection || 'unknown'}\nCount: ${result.count || result.items.length}\n\nResults:\n${JSON.stringify(result.items, null, 2)}`;
    }

    if (toolName.startsWith(DATA_COMMANDS.READ) && result.data) {
      return `Collection: ${result.collection || 'unknown'}\nID: ${result.id || 'unknown'}\n\nData:\n${JSON.stringify(result.data, null, 2)}`;
    }

    if (toolName === 'code/read' && result.content) {
      const lineRange = result.startLine && result.endLine
        ? ` (lines ${result.startLine}-${result.endLine})`
        : '';
      return `Path: ${result.path || 'unknown'}${lineRange}\n\n${result.content}`;
    }

    if (toolName === 'collaboration/chat/export' && result.markdown) {
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
Format: <tool name="command/name"><param>value</param></tool>
Run "help" or "list" to see command parameters.

${toolDescriptions}`;
  }
}
