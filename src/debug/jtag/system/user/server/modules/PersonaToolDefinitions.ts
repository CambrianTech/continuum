/**
 * PersonaToolDefinitions.ts
 *
 * Type definitions and interfaces for PersonaUser tool calling system.
 * Defines what tools are available, their parameters, and how they're used.
 *
 * Part of Phase 3A: Tool Calling Foundation
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';

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
 * Built-in tool definitions
 */
export const BUILTIN_TOOLS: Record<string, ToolDefinition> = {
  'read': {
    name: 'read',
    description: 'Read contents of a file from the filesystem',
    category: 'file',
    permissions: ['file:read'],
    parameters: {
      type: 'object',
      properties: {
        filepath: {
          type: 'string',
          description: 'Absolute path to the file to read',
          required: true
        },
        offset: {
          type: 'number',
          description: 'Line number to start reading from (optional)',
          required: false
        },
        limit: {
          type: 'number',
          description: 'Number of lines to read (optional)',
          required: false
        }
      },
      required: ['filepath']
    },
    examples: [
      {
        description: 'Read entire file',
        params: {
          filepath: '/path/to/file.ts'
        },
        expectedResult: 'File contents with line numbers'
      },
      {
        description: 'Read specific range',
        params: {
          filepath: '/path/to/file.ts',
          offset: 100,
          limit: 50
        },
        expectedResult: 'Lines 100-150 from file'
      }
    ]
  },

  'grep': {
    name: 'grep',
    description: 'Search for patterns in files using regex',
    category: 'code',
    permissions: ['code:search'],
    parameters: {
      type: 'object',
      properties: {
        pattern: {
          type: 'string',
          description: 'Regular expression pattern to search for',
          required: true
        },
        path: {
          type: 'string',
          description: 'File or directory to search in',
          required: false
        },
        glob: {
          type: 'string',
          description: 'Glob pattern to filter files (e.g., "*.ts")',
          required: false
        },
        output_mode: {
          type: 'string',
          description: 'Output format: content, files_with_matches, count',
          enum: ['content', 'files_with_matches', 'count'],
          required: false,
          default: 'files_with_matches'
        },
        case_insensitive: {
          type: 'boolean',
          description: 'Perform case-insensitive search',
          required: false,
          default: false
        }
      },
      required: ['pattern']
    },
    examples: [
      {
        description: 'Find all files containing pattern',
        params: {
          pattern: 'PersonaUser',
          glob: '*.ts'
        },
        expectedResult: 'List of files containing PersonaUser'
      },
      {
        description: 'Search with content',
        params: {
          pattern: 'interface.*User',
          output_mode: 'content',
          glob: '*.ts'
        },
        expectedResult: 'Matching lines with context'
      }
    ]
  },

  'bash': {
    name: 'bash',
    description: 'Execute shell commands (read-only operations preferred)',
    category: 'system',
    permissions: ['system:execute'],
    parameters: {
      type: 'object',
      properties: {
        command: {
          type: 'string',
          description: 'Shell command to execute',
          required: true
        },
        timeout: {
          type: 'number',
          description: 'Timeout in milliseconds (max 600000)',
          required: false,
          default: 120000
        }
      },
      required: ['command']
    },
    examples: [
      {
        description: 'List files',
        params: {
          command: 'ls -la'
        },
        expectedResult: 'Directory listing'
      },
      {
        description: 'Check git status',
        params: {
          command: 'git status'
        },
        expectedResult: 'Git status output'
      }
    ]
  },

  'screenshot': {
    name: 'screenshot',
    description: 'Capture screenshot of UI element or entire page',
    category: 'media',
    permissions: ['ui:screenshot'],
    parameters: {
      type: 'object',
      properties: {
        querySelector: {
          type: 'string',
          description: 'CSS selector for element to capture (optional, defaults to body)',
          required: false,
          default: 'body'
        },
        filename: {
          type: 'string',
          description: 'Output filename (optional)',
          required: false
        }
      },
      required: []
    },
    examples: [
      {
        description: 'Capture entire page',
        params: {},
        expectedResult: 'Screenshot saved to file'
      },
      {
        description: 'Capture specific widget',
        params: {
          querySelector: 'chat-widget',
          filename: 'chat-debug.png'
        },
        expectedResult: 'Screenshot of chat widget'
      }
    ]
  }
};

/**
 * Get all available tools
 */
export function getAllToolDefinitions(): ToolDefinition[] {
  return Object.values(BUILTIN_TOOLS);
}

/**
 * Get tool by name
 */
export function getToolDefinition(name: string): ToolDefinition | null {
  return BUILTIN_TOOLS[name] || null;
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
