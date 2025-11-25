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
  refreshToolDefinitions,
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
    console.log('[ToolRegistry] Initialized - refreshing tool definitions from Commands system');
    // Async refresh - fire and forget (cache will populate in background)
    refreshToolDefinitions().catch(err => {
      console.error('[ToolRegistry] Failed to refresh tool definitions:', err);
    });
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
   * Execute tool command generically via Commands system
   * NO HARDCODED MAPPINGS - just execute the command directly
   */
  private async executeToolCommand(
    toolName: string,
    params: Record<string, unknown>,
    context: JTAGContext
  ): Promise<{ content?: string; media?: MediaItem[] }> {
    // Execute command directly - toolName IS the command name
    const result = await Commands.execute(toolName, params);

    // Generic result formatting
    // Commands return whatever format they want - we just stringify it
    if (result && typeof result === 'object') {
      // Check for media (screenshots, images, etc.)
      const media: MediaItem[] = [];

      // If result has base64 image data, extract it as media
      if ('base64' in result && typeof result.base64 === 'string') {
        media.push({
          type: 'image',
          base64: result.base64,
          mimeType: 'image/png',
          description: 'Tool result image'
        });
      }

      // Convert result to string content
      const content = JSON.stringify(result, null, 2);

      return {
        content,
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
