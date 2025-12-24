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
 */
export function formatAllToolsForAI(): string {
  let output = 'Available Tools:\n\n';

  for (const tool of getAllToolDefinitions()) {
    output += formatToolForAI(tool) + '\n---\n\n';
  }

  output += `
Usage Format (Anthropic Claude XML style):
<tool_use>
  <tool_name>read</tool_name>
  <parameters>
    <filepath>/path/to/file.ts</filepath>
  </parameters>
</tool_use>

When you need information, use tools instead of making assumptions.
Examples:
- Unknown file content? Use 'read' tool
- Need to find code? Use 'grep' tool
- Want to verify UI? Use 'screenshot' tool
- Need to check system state? Use 'bash' tool (read-only operations)
`;

  return output;
}
