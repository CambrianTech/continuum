/**
 * ToolRegistry.ts
 *
 * Central registry for tool execution used by PersonaToolExecutor.
 * Bridges between PersonaUser tool calls and actual command execution.
 *
 * Part of Phase 3A: Tool Calling Foundation
 * Integrates with PersonaToolDefinitions and PersonaToolRegistry
 */

import { Commands } from '@system/core/shared/Commands';
import type { UUID } from '@types/CrossPlatformUUID';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { MediaItem } from '@system/data/entities/ChatMessageEntity';
import {
  getAllToolDefinitions,
  getToolDefinition,
  validateToolParameters,
  type ToolDefinition
} from '@system/user/server/modules/PersonaToolDefinitions';

/**
 * Tool execution result from ToolRegistry
 */
export interface ToolRegistryResult {
  toolName: string;
  success: boolean;
  content?: string;
  media?: MediaItem[];
  error?: string;
}

/**
 * Tool info for listing
 */
export interface ToolInfo {
  name: string;
  description: string;
  category: string;
}

/**
 * Central registry for tool discovery and execution
 * Used by PersonaToolExecutor to execute tools from AI responses
 */
export class ToolRegistry {
  private static instance: ToolRegistry | null = null;

  private constructor() {
    console.log('[ToolRegistry] Initialized');
  }

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
   * Execute a tool with parameters
   *
   * @param toolName - Name of the tool to execute
   * @param params - Tool parameters as Record<string, string> (from XML parsing)
   * @param sessionId - Session ID of the PersonaUser
   * @param contextId - Context ID (room/conversation)
   * @param context - JTAG context with caller info
   * @returns ToolRegistryResult with content and optional media
   */
  async executeTool(
    toolName: string,
    params: Record<string, string>,
    sessionId: UUID,
    contextId: UUID,
    context: JTAGContext
  ): Promise<ToolRegistryResult> {
    // 1. Validate tool exists
    const toolDef = getToolDefinition(toolName);
    if (!toolDef) {
      return {
        toolName,
        success: false,
        error: `Tool '${toolName}' not found. Available tools: ${this.getAllTools().map(t => t.name).join(', ')}`
      };
    }

    // 2. Convert string params to proper types based on tool definition
    const typedParams = this.convertParameterTypes(params, toolDef);

    // 3. Validate parameters
    const validation = validateToolParameters(toolName, typedParams);
    if (!validation.valid) {
      return {
        toolName,
        success: false,
        error: `Invalid parameters: ${validation.errors.join(', ')}`
      };
    }

    // 4. Execute tool via Commands
    try {
      const result = await this.executeToolCommand(toolName, typedParams, context);
      return {
        toolName,
        success: true,
        content: result.content,
        media: result.media
      };
    } catch (error) {
      return {
        toolName,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Execute tool command and return result with media
   */
  private async executeToolCommand(
    toolName: string,
    params: Record<string, unknown>,
    context: JTAGContext
  ): Promise<{ content?: string; media?: MediaItem[] }> {
    switch (toolName) {
      case 'read':
        return this.executeRead(params);

      case 'grep':
        return this.executeGrep(params);

      case 'bash':
        return this.executeBash(params);

      case 'screenshot':
        return this.executeScreenshot(params);

      default:
        throw new Error(`Tool '${toolName}' has no executor implementation`);
    }
  }

  /**
   * Execute 'read' tool
   */
  private async executeRead(params: Record<string, unknown>): Promise<{ content?: string }> {
    const result = await Commands.execute('file/read', {
      filepath: params.filepath as string,
      offset: params.offset as number | undefined,
      limit: params.limit as number | undefined
    });

    // Format file content with line numbers
    if (result && typeof result === 'object' && 'content' in result) {
      return { content: result.content as string };
    }

    return { content: String(result) };
  }

  /**
   * Execute 'grep' tool
   */
  private async executeGrep(params: Record<string, unknown>): Promise<{ content?: string }> {
    const result = await Commands.execute('code/pattern-search', {
      pattern: params.pattern as string,
      path: params.path as string | undefined,
      glob: params.glob as string | undefined,
      output_mode: params.output_mode as string | undefined,
      case_insensitive: params.case_insensitive as boolean | undefined
    });

    // Format search results
    if (result && typeof result === 'object' && 'matches' in result) {
      const matches = result.matches as Array<{ file: string; line: number; content: string }>;
      if (matches.length === 0) {
        return { content: 'No matches found' };
      }

      const content = matches
        .map(m => `${m.file}:${m.line}: ${m.content}`)
        .join('\n');
      return { content };
    }

    return { content: String(result) };
  }

  /**
   * Execute 'bash' tool
   */
  private async executeBash(params: Record<string, unknown>): Promise<{ content?: string }> {
    const result = await Commands.execute('bash/execute', {
      command: params.command as string,
      timeout: params.timeout as number | undefined
    });

    // Format bash output
    if (result && typeof result === 'object' && 'stdout' in result) {
      const stdout = result.stdout as string;
      const stderr = result.stderr as string;
      let content = stdout;
      if (stderr) {
        content += `\nSTDERR:\n${stderr}`;
      }
      return { content };
    }

    return { content: String(result) };
  }

  /**
   * Execute 'screenshot' tool
   */
  private async executeScreenshot(
    params: Record<string, unknown>
  ): Promise<{ content?: string; media?: MediaItem[] }> {
    const result = await Commands.execute('screenshot', {
      querySelector: params.querySelector as string | undefined,
      filename: params.filename as string | undefined
    });

    // Screenshot returns file path and optionally base64 data
    if (result && typeof result === 'object' && 'filepath' in result) {
      const filepath = result.filepath as string;
      const base64 = ('base64' in result ? result.base64 : null) as string | null;

      // Create media item for vision-capable AIs
      const media: MediaItem[] = [];
      if (base64) {
        media.push({
          type: 'image',
          source: {
            type: 'base64',
            media_type: 'image/png',
            data: base64
          },
          caption: `Screenshot: ${filepath}`
        });
      }

      return {
        content: `Screenshot saved to: ${filepath}`,
        media: media.length > 0 ? media : undefined
      };
    }

    return { content: String(result) };
  }

  /**
   * Convert string parameters to proper types based on tool definition
   */
  private convertParameterTypes(
    params: Record<string, string>,
    toolDef: ToolDefinition
  ): Record<string, unknown> {
    const converted: Record<string, unknown> = {};

    for (const [key, value] of Object.entries(params)) {
      const paramDef = toolDef.parameters.properties[key];
      if (!paramDef) {
        // Unknown parameter, keep as string
        converted[key] = value;
        continue;
      }

      switch (paramDef.type) {
        case 'number':
          converted[key] = Number(value);
          break;

        case 'boolean':
          converted[key] = value === 'true' || value === '1';
          break;

        case 'array':
          try {
            converted[key] = JSON.parse(value);
          } catch {
            converted[key] = value.split(',').map(v => v.trim());
          }
          break;

        case 'object':
          try {
            converted[key] = JSON.parse(value);
          } catch {
            converted[key] = value;
          }
          break;

        default:
          converted[key] = value;
      }
    }

    return converted;
  }

  /**
   * Get all available tools
   */
  getAllTools(): ToolInfo[] {
    return getAllToolDefinitions().map(tool => ({
      name: tool.name,
      description: tool.description,
      category: tool.category
    }));
  }

  /**
   * Get tool by name
   */
  getTool(name: string): ToolInfo | null {
    const tool = getToolDefinition(name);
    if (!tool) return null;

    return {
      name: tool.name,
      description: tool.description,
      category: tool.category
    };
  }
}
