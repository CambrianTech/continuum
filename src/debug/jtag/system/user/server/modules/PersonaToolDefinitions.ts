/**
 * PersonaToolDefinitions.ts
 *
 * DYNAMIC tool discovery system for PersonaUser tool calling.
 * Queries the Commands system via 'list' command to discover all available tools.
 * No more hardcoded tools - everything is discovered dynamically!
 *
 * Part of Phase 3A: Tool Calling Foundation
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { Commands } from '../../../core/shared/Commands';
import type { CommandSignature, ListResult } from '../../../../commands/list/shared/ListTypes';
import { ToolRegistry } from '../../../tools/server/ToolRegistry';

/**
 * Result from tool execution
 */
export interface ToolResult {
  success: boolean;
  data?: unknown;
  error?: string;
  metadata?: {
    executionTime?: number;
    toolName: string;
    timestamp: string;
  };
}

/**
 * Error from tool execution
 */
export interface ToolError {
  toolName: string;
  errorType: 'permission' | 'validation' | 'execution' | 'not_found';
  message: string;
  details?: unknown;
}

/**
 * Parameter schema for a tool
 */
export interface ToolParameterSchema {
  type: 'object';
  properties: Record<string, ParameterDefinition>;
  required: string[];
}

/**
 * Individual parameter definition
 */
export interface ParameterDefinition {
  type: 'string' | 'number' | 'boolean' | 'array' | 'object';
  description: string;
  required?: boolean;
  default?: unknown;
  enum?: string[];
  pattern?: string;
  items?: ParameterDefinition; // For array types
}

/**
 * Example of tool usage
 */
export interface ToolExample {
  description: string;
  params: Record<string, unknown>;
  expectedResult?: string;
}

/**
 * Complete tool definition
 */
export interface ToolDefinition {
  name: string;
  description: string;
  parameters: ToolParameterSchema;
  permissions: string[];
  examples: ToolExample[];
  category: 'file' | 'code' | 'system' | 'media' | 'data';
}

/**
 * Tool execution context
 */
export interface ToolExecutionContext {
  personaId: UUID;
  sessionId: UUID;
  timestamp: string;
  permissions: string[];
}

/**
 * Tool cache - populated dynamically from Commands system
 * Refreshed on initialization and periodically
 */
let toolCache: ToolDefinition[] = [];
let lastRefreshTime = 0;
const CACHE_TTL_MS = 5 * 60 * 1000; // 5 minutes

/**
 * Module-level logger - can be set via setToolDefinitionsLogger()
 */
let moduleLogger: ((message: string) => void) | null = null;

/**
 * Set the logger for PersonaToolDefinitions module
 */
export function setToolDefinitionsLogger(logger: (message: string) => void): void {
  moduleLogger = logger;
}

function log(message: string): void {
  if (moduleLogger) {
    moduleLogger(message);
  }
  // Silent if no logger set - this is initialization-time code
}

/**
 * Refresh tool definitions from Commands system
 * Queries 'list' command to discover ALL available commands dynamically
 *
 * TODO: Add access level filtering based on command metadata
 * Commands should declare their own accessLevel (e.g., 'ai-tool', 'internal', 'system')
 * Then we filter: toolCache = listResult.commands.filter(cmd => cmd.accessLevel === 'ai-tool')
 * This way commands control their own visibility - no hardcoding!
 */
export async function refreshToolDefinitions(): Promise<void> {
  try {
    log('Refreshing tool cache from Commands system...');

    // Query list command to discover all available commands
    const result = await Commands.execute('list', {}) as unknown as ListResult;

    if (!result.success || !result.commands) {
      log(`❌ Failed to refresh tools: ${result.error}`);
      return;
    }

    // Convert ALL commands from list result
    // TODO: Filter based on cmd.accessLevel or cmd.permissions when that metadata exists
    toolCache = result.commands.map((cmd: CommandSignature) => convertCommandToTool(cmd));

    // Also include built-in meta-tools from ToolRegistry (search_tools, list_tools, etc.)
    // These are essential for personas to discover tools without loading all into context
    try {
      const registry = ToolRegistry.getInstance();
      // Only add if registry is initialized (has tools from list command)
      const registryTools = registry.getAllTools();
      const metaTools = registryTools.filter(t => t.category === 'meta');

      for (const metaTool of metaTools) {
        // Convert ToolRegistry format to our ToolDefinition format
        const properties: Record<string, ParameterDefinition> = {};
        const required: string[] = [];

        for (const [paramName, paramInfo] of Object.entries(metaTool.parameters)) {
          properties[paramName] = {
            type: paramInfo.type as any,
            description: paramInfo.description || `${paramName} parameter`,
            required: paramInfo.required
          };
          if (paramInfo.required) {
            required.push(paramName);
          }
        }

        // Add if not already in cache (avoid duplicates)
        if (!toolCache.find(t => t.name === metaTool.name)) {
          toolCache.push({
            name: metaTool.name,
            description: metaTool.description,
            category: 'system',  // Meta-tools are system tools
            permissions: ['system:execute'],
            parameters: { type: 'object', properties, required },
            examples: []
          });
        }
      }

      log(`Added ${metaTools.length} meta-tools from ToolRegistry`);
    } catch (registryError) {
      // ToolRegistry not initialized yet - that's fine, meta-tools will be added on next refresh
      log(`ToolRegistry not ready (will retry): ${registryError}`);
    }

    lastRefreshTime = Date.now();
    log(`Refreshed ${toolCache.length} tools from Commands system`);
  } catch (error) {
    log(`❌ Error refreshing tools: ${error}`);
  }
}

/**
 * Convert CommandSignature to ToolDefinition
 */
function convertCommandToTool(cmd: CommandSignature): ToolDefinition {
  // Determine category from command name prefix
  const category = inferCategoryFromName(cmd.name);

  // Convert params to our ToolParameterSchema format
  const properties: Record<string, ParameterDefinition> = {};
  const required: string[] = [];

  if (cmd.params) {
    for (const [paramName, paramInfo] of Object.entries(cmd.params)) {
      properties[paramName] = {
        type: paramInfo.type as any,  // Trust the type from command signature
        description: paramInfo.description || `${paramName} parameter`,
        required: paramInfo.required
      };

      if (paramInfo.required) {
        required.push(paramName);
      }
    }
  }

  return {
    name: cmd.name,
    description: cmd.description || `Execute ${cmd.name} command`,
    category,
    permissions: [category + ':execute'],
    parameters: {
      type: 'object',
      properties,
      required
    },
    examples: []  // Could add examples in future
  };
}

/**
 * Infer tool category from command name
 */
function inferCategoryFromName(name: string): ToolDefinition['category'] {
  if (name.startsWith('code/') || name.startsWith('git/')) return 'code';
  if (name.startsWith('file/')) return 'file';
  if (name.startsWith('data/') || name.startsWith('memory/')) return 'data';
  if (name.startsWith('media/') || name.includes('screenshot')) return 'media';
  return 'system';  // Default
}

/**
 * Get all available tools (from cache)
 * Auto-refreshes if cache is empty or stale
 */
export function getAllToolDefinitions(): ToolDefinition[] {
  // Auto-refresh if cache is stale or empty
  if (toolCache.length === 0 || (Date.now() - lastRefreshTime) > CACHE_TTL_MS) {
    // Trigger async refresh in background (first call may return empty, subsequent calls will have tools)
    refreshToolDefinitions().catch(err => {
      log(`❌ Auto-refresh failed: ${err}`);
    });
  }
  return toolCache;
}

/**
 * Get all available tools with guaranteed initialization
 * Blocks until tools are loaded (use for critical paths)
 */
export async function getAllToolDefinitionsAsync(): Promise<ToolDefinition[]> {
  if (toolCache.length === 0 || (Date.now() - lastRefreshTime) > CACHE_TTL_MS) {
    await refreshToolDefinitions();
  }
  return toolCache;
}

/**
 * Get tool by name (from cache)
 */
export function getToolDefinition(name: string): ToolDefinition | null {
  return toolCache.find(tool => tool.name === name) || null;
}

/**
 * Validate tool parameters against schema
 */
export function validateToolParameters(
  toolName: string,
  params: Record<string, unknown>
): { valid: boolean; errors: string[] } {
  const tool = getToolDefinition(toolName);
  if (!tool) {
    return { valid: false, errors: [`Tool '${toolName}' not found`] };
  }

  const errors: string[] = [];

  // Check required parameters
  for (const requiredParam of tool.parameters.required) {
    if (!(requiredParam in params)) {
      errors.push(`Missing required parameter: ${requiredParam}`);
    }
  }

  // Validate parameter types
  for (const [paramName, paramValue] of Object.entries(params)) {
    const paramDef = tool.parameters.properties[paramName];
    if (!paramDef) {
      errors.push(`Unknown parameter: ${paramName}`);
      continue;
    }

    const actualType = Array.isArray(paramValue) ? 'array' : typeof paramValue;
    if (actualType !== paramDef.type && paramValue !== null && paramValue !== undefined) {
      errors.push(`Parameter '${paramName}' should be ${paramDef.type}, got ${actualType}`);
    }

    // Validate enum values
    if (paramDef.enum && !paramDef.enum.includes(paramValue as string)) {
      errors.push(`Parameter '${paramName}' must be one of: ${paramDef.enum.join(', ')}`);
    }
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Format tool definition for AI consumption
 */
export function formatToolForAI(tool: ToolDefinition): string {
  let output = `Tool: ${tool.name}\n`;
  output += `Description: ${tool.description}\n`;
  output += `Category: ${tool.category}\n\n`;

  output += `Parameters:\n`;
  for (const [paramName, paramDef] of Object.entries(tool.parameters.properties)) {
    const required = tool.parameters.required.includes(paramName) ? ' (required)' : ' (optional)';
    output += `  - ${paramName}${required}: ${paramDef.description}\n`;
    if (paramDef.default !== undefined) {
      output += `    Default: ${paramDef.default}\n`;
    }
    if (paramDef.enum) {
      output += `    Options: ${paramDef.enum.join(', ')}\n`;
    }
  }

  output += `\nExamples:\n`;
  for (const example of tool.examples) {
    output += `  ${example.description}:\n`;
    output += `    ${JSON.stringify(example.params, null, 2)}\n`;
  }

  return output;
}

/**
 * Format all tools for AI system prompt
 * Shows ALL tools organized by category so AIs know their full capabilities
 */
export function formatAllToolsForAI(): string {
  const tools = getAllToolDefinitions();

  // Group tools by category
  const byCategory = new Map<string, ToolDefinition[]>();
  for (const tool of tools) {
    const category = tool.category || 'other';
    if (!byCategory.has(category)) {
      byCategory.set(category, []);
    }
    byCategory.get(category)!.push(tool);
  }

  // Sort categories alphabetically
  const sortedCategories = Array.from(byCategory.keys()).sort();

  let output = `=== YOUR TOOL CAPABILITIES ===
You have ${tools.length} tools available. Here they ALL are, organized by category:

`;

  // List ALL tools by category (compact: name - short description)
  for (const category of sortedCategories) {
    const categoryTools = byCategory.get(category)!;
    output += `📁 ${category.toUpperCase()} (${categoryTools.length}):\n`;
    for (const tool of categoryTools.sort((a, b) => a.name.localeCompare(b.name))) {
      // Truncate description to 60 chars for compact display
      const desc = tool.description.length > 60
        ? tool.description.slice(0, 57) + '...'
        : tool.description;
      output += `  ${tool.name} - ${desc}\n`;
    }
    output += '\n';
  }

  // Show essential tools with full details
  const essentialTools = tools.filter(t =>
    ['screenshot', 'help', 'collaboration/chat/send', 'collaboration/wall/write',
     'development/code/read', 'development/code/pattern-search'].includes(t.name)
  );

  output += `=== FREQUENTLY USED TOOLS (with parameters) ===\n`;

  for (const tool of essentialTools) {
    output += `\n${tool.name} - ${tool.description}\n`;
    const params = Object.entries(tool.parameters.properties);
    if (params.length > 0) {
      for (const [name, def] of params) {
        const req = tool.parameters.required.includes(name) ? ' (required)' : '';
        output += `  <${name}>${def.description}${req}</${name}>\n`;
      }
    }
  }

  output += `
=== HOW TO USE TOOLS ===

For any tool above, use this format:
<tool_use>
  <tool_name>TOOL_NAME</tool_name>
  <parameters>
    <param1>value1</param1>
    <param2>value2</param2>
  </parameters>
</tool_use>

Example - Take a screenshot:
<tool_use>
  <tool_name>screenshot</tool_name>
  <parameters>
    <querySelector>chat-widget</querySelector>
  </parameters>
</tool_use>

For help on any specific tool, use:
<tool_use>
  <tool_name>help</tool_name>
  <parameters>
    <path>TOOL_NAME</path>
  </parameters>
</tool_use>
`;

  return output;
}
