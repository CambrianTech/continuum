/**
 * PersonaToolExecutor - Handles tool calling for PersonaUser
 *
 * Parses tool calls from AI responses, executes them via Commands.execute(),
 * and formats results for injection back into conversation.
 *
 * DYNAMIC TOOL DISCOVERY:
 * - Uses ToolRegistry to discover available commands
 * - No hardcoded tool list - automatically supports new commands
 * - Special handlers only for edge cases (git, file/save restrictions, code/read)
 */

import { Commands } from '../../../core/shared/Commands';
import { CodeDaemon } from '../../../../daemons/code-daemon/shared/CodeDaemon';
import { CognitionLogger } from './cognition/CognitionLogger';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import { execSync } from 'child_process';
import { ToolRegistry } from '../../../tools/server/ToolRegistry';

/**
 * Parsed tool call from AI response
 */
export interface ToolCall {
  toolName: string;
  parameters: Record<string, string>;
}

/**
 * Tool execution result
 */
export interface ToolResult {
  toolName: string;
  success: boolean;
  content?: string;
  error?: string;
}

/**
 * Tool handler function signature
 */
type ToolHandler = (params: Record<string, string>) => Promise<ToolResult>;

/**
 * PersonaToolExecutor - Dynamic tool execution for PersonaUser
 */
export class PersonaToolExecutor {
  private toolHandlers: Map<string, ToolHandler> = new Map();
  private personaId: UUID;
  private personaName: string;
  private toolRegistry: ToolRegistry;

  constructor(personaId: UUID, personaName: string) {
    this.personaId = personaId;
    this.personaName = personaName;
    this.toolRegistry = ToolRegistry.getInstance();
    this.registerSpecialHandlers();
  }

  /**
   * Register special handlers for edge cases
   * Most tools use universal ToolRegistry handler
   */
  private registerSpecialHandlers(): void {
    // code/read - Uses CodeDaemon for better performance
    this.registerTool('code/read', async (params) => {
      const options: { startLine?: number; endLine?: number } = {};

      if (params.startLine) {
        options.startLine = parseInt(params.startLine, 10);
      }
      if (params.endLine) {
        options.endLine = parseInt(params.endLine, 10);
      }

      const result = await CodeDaemon.readFile(params.path, options);

      if (result.success && result.content) {
        const lineRange = options.startLine && options.endLine
          ? ` (lines ${options.startLine}-${options.endLine})`
          : '';
        return {
          toolName: 'code/read',
          success: true,
          content: `Path: ${params.path}${lineRange}\n\n${result.content}`
        };
      } else {
        // Log detailed error for debugging
        console.error(`❌ code/read failed for path="${params.path}":`, {
          error: result.error,
          success: result.success,
          metadata: result.metadata
        });
        return {
          toolName: 'code/read',
          success: false,
          error: result.error || 'Unknown error'
        };
      }
    });

    // list - List all available commands
    this.registerTool('list', async () => {
      const result = await Commands.execute('list', {}) as unknown as {
        commands?: Array<{name: string; description: string; category: string}>;
        success: boolean;
        error?: string
      };

      if (result.success && result.commands) {
        // Format CommandSignature objects as readable strings
        const formattedCommands = result.commands
          .map(cmd => `${cmd.name} - ${cmd.description} [${cmd.category}]`)
          .join('\n');
        return {
          toolName: 'list',
          success: true,
          content: formattedCommands
        };
      } else {
        return {
          toolName: 'list',
          success: false,
          error: result.error || 'Failed to list commands'
        };
      }
    });

    // system/daemons - Show daemon status
    this.registerTool('system/daemons', async () => {
      const result = await Commands.execute('system/daemons', {}) as unknown as {
        daemons?: Array<{name: string; status: string}>;
        success: boolean;
        error?: string
      };

      if (result.success && result.daemons) {
        const daemonsList = result.daemons
          .map(d => `${d.name}: ${d.status}`)
          .join('\n');
        return {
          toolName: 'system/daemons',
          success: true,
          content: daemonsList
        };
      } else {
        return {
          toolName: 'system/daemons',
          success: false,
          error: result.error || 'Failed to query daemons'
        };
      }
    });

    // data/list - Query database collections
    this.registerTool('data/list', async (params) => {
      try {
        const filter = params.filter ? JSON.parse(params.filter) : undefined;
        const orderBy = params.orderBy ? JSON.parse(params.orderBy) : undefined;
        const limit = params.limit ? parseInt(params.limit, 10) : 50;

        const result: any = await Commands.execute('data/list', {
          collection: params.collection,
          filter,
          orderBy,
          limit
        } as any);

        if (result.success && result.items) {
          const itemsJson = JSON.stringify(result.items, null, 2);
          return {
            toolName: 'data/list',
            success: true,
            content: `Collection: ${params.collection}\nCount: ${result.count}\n\nResults:\n${itemsJson}`
          };
        } else {
          return {
            toolName: 'data/list',
            success: false,
            error: result.error || 'Failed to query collection'
          };
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        return {
          toolName: 'data/list',
          success: false,
          error: `Parse error: ${errorMessage}`
        };
      }
    });

    // data/read - Read single record by ID
    this.registerTool('data/read', async (params) => {
      const result: any = await Commands.execute('data/read', {
        collection: params.collection,
        id: params.id
      } as any);

      if (result.success && result.found && result.data) {
        const dataJson = JSON.stringify(result.data, null, 2);
        return {
          toolName: 'data/read',
          success: true,
          content: `Collection: ${params.collection}\nID: ${params.id}\n\nData:\n${dataJson}`
        };
      } else if (result.success && !result.found) {
        return {
          toolName: 'data/read',
          success: false,
          error: `Record not found: ${params.id} in ${params.collection}`
        };
      } else {
        return {
          toolName: 'data/read',
          success: false,
          error: result.error || 'Failed to read record'
        };
      }
    });

    // data/create - Create new record
    this.registerTool('data/create', async (params) => {
      try {
        const data = params.data ? JSON.parse(params.data) : {};

        const result: any = await Commands.execute('data/create', {
          collection: params.collection,
          data
        } as any);

        if (result.success && result.data) {
          const dataJson = JSON.stringify(result.data, null, 2);
          return {
            toolName: 'data/create',
            success: true,
            content: `Created in ${params.collection}:\n${dataJson}`
          };
        } else {
          return {
            toolName: 'data/create',
            success: false,
            error: result.error || 'Failed to create record'
          };
        }
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        return {
          toolName: 'data/create',
          success: false,
          error: `Parse error: ${errorMessage}`
        };
      }
    });

    // chat/export - Export chat messages to markdown
    this.registerTool('chat/export', async (params) => {
      const limit = params.limit ? parseInt(params.limit, 10) : 50;
      const includeSystem = params.includeSystem === 'true';
      const includeTests = params.includeTests === 'true';
      const includeThreading = params.includeThreading === 'true';

      const result: any = await Commands.execute('chat/export', {
        room: params.room,
        afterMessageId: params.afterMessageId,
        afterTimestamp: params.afterTimestamp,
        limit,
        includeSystem,
        includeTests,
        includeThreading,
        filter: params.filter ? JSON.parse(params.filter) : undefined
      } as any);

      if (result.success && result.markdown) {
        return {
          toolName: 'chat/export',
          success: true,
          content: `Exported ${result.messageCount} messages from ${params.room || 'all rooms'}:\n\n${result.markdown}`
        };
      } else {
        return {
          toolName: 'chat/export',
          success: false,
          error: result.error || 'Failed to export chat messages'
        };
      }
    });

    // cognition/inspect - Query own cognitive logs (friendly wrapper for data/list)
    this.registerTool('cognition/inspect', async (params) => {
      // Map friendly type names to collection names
      const typeMap: Record<string, string> = {
        'tools': 'tool_execution_logs',
        'decisions': 'adapter_decision_logs',
        'responses': 'response_generation_logs',
        'plans': 'cognition_plan_records',
        'plan-steps': 'cognition_plan_step_executions',
        'state': 'cognition_self_state_updates',
        'memory': 'cognition_memory_operations',
        'reasoning': 'adapter_reasoning_logs',
        'replans': 'cognition_plan_replans'
      };

      const type = params.type || 'tools';
      const collection = typeMap[type] || 'tool_execution_logs';
      const limit = params.limit ? parseInt(params.limit, 10) : 10;

      // Auto-filter to this persona's ID
      const filter = params.filter
        ? JSON.parse(params.filter)
        : { personaId: this.personaId };

      const result: any = await Commands.execute('data/list', {
        collection,
        filter,
        orderBy: params.orderBy ? JSON.parse(params.orderBy) : [{ field: 'startedAt', direction: 'desc' }],
        limit
      } as any);

      if (result.success && result.items) {
        const itemsJson = JSON.stringify(result.items, null, 2);
        return {
          toolName: 'cognition/inspect',
          success: true,
          content: `${type} logs (${result.count} total):\n\n${itemsJson}`
        };
      } else {
        return {
          toolName: 'cognition/inspect',
          success: false,
          error: result.error || 'Failed to query cognitive logs'
        };
      }
    });

    // file/save - Write file to filesystem (with safety restrictions)
    this.registerTool('file/save', async (params) => {
      // Safety: Only allow writing to safe directories
      const allowedDirectories = [
        '/tmp/',
        '/private/tmp/',
        '.continuum/jtag/'
      ];

      const filepath = params.filepath;
      const isAllowed = allowedDirectories.some(dir => filepath.startsWith(dir));

      if (!isAllowed) {
        return {
          toolName: 'file/save',
          success: false,
          error: `Permission denied: file/save is restricted to safe directories only: ${allowedDirectories.join(', ')}`
        };
      }

      const result: any = await Commands.execute('file/save', {
        filepath: params.filepath,
        content: params.content,
        createDirs: params.createDirs !== 'false'
      } as any);

      if (result.success) {
        return {
          toolName: 'file/save',
          success: true,
          content: `${result.created ? 'Created' : 'Updated'} file: ${result.filepath}\nBytes written: ${result.bytesWritten}`
        };
      } else {
        return {
          toolName: 'file/save',
          success: false,
          error: result.error || 'Failed to save file'
        };
      }
    });

    // git/status - Show git repository status
    this.registerTool('git/status', async () => {
      try {
        const output = execSync('git status', {
          cwd: process.cwd(),
          encoding: 'utf-8',
          maxBuffer: 1024 * 1024
        });
        return {
          toolName: 'git/status',
          success: true,
          content: output
        };
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        return {
          toolName: 'git/status',
          success: false,
          error: `Failed to run git status: ${errorMessage}`
        };
      }
    });

    // git/diff - Show git diff (unstaged changes)
    this.registerTool('git/diff', async (params) => {
      try {
        const file = params.file || '';
        const staged = params.staged === 'true';
        const command = staged
          ? `git diff --cached ${file}`.trim()
          : `git diff ${file}`.trim();

        const output = execSync(command, {
          cwd: process.cwd(),
          encoding: 'utf-8',
          maxBuffer: 10 * 1024 * 1024 // 10MB for large diffs
        });
        return {
          toolName: 'git/diff',
          success: true,
          content: output || '(No changes)'
        };
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        return {
          toolName: 'git/diff',
          success: false,
          error: `Failed to run git diff: ${errorMessage}`
        };
      }
    });

    // git/log - Show git commit history
    this.registerTool('git/log', async (params) => {
      try {
        const limit = params.limit ? parseInt(params.limit, 10) : 10;
        const oneline = params.oneline === 'true';
        const format = oneline ? '--oneline' : '--pretty=format:%h - %an, %ar : %s';
        const command = `git log ${format} -n ${limit}`;

        const output = execSync(command, {
          cwd: process.cwd(),
          encoding: 'utf-8',
          maxBuffer: 1024 * 1024
        });
        return {
          toolName: 'git/log',
          success: true,
          content: output
        };
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        return {
          toolName: 'git/log',
          success: false,
          error: `Failed to run git log: ${errorMessage}`
        };
      }
    });

    // git/show - Show details of a specific commit
    this.registerTool('git/show', async (params) => {
      try {
        const commit = params.commit || 'HEAD';
        const stat = params.stat === 'true' ? '--stat' : '';
        const command = `git show ${stat} ${commit}`.trim();

        const output = execSync(command, {
          cwd: process.cwd(),
          encoding: 'utf-8',
          maxBuffer: 10 * 1024 * 1024
        });
        return {
          toolName: 'git/show',
          success: true,
          content: output
        };
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        return {
          toolName: 'git/show',
          success: false,
          error: `Failed to run git show: ${errorMessage}`
        };
      }
    });
  }

  /**
   * Register a tool handler dynamically
   */
  registerTool(toolName: string, handler: ToolHandler): void {
    this.toolHandlers.set(toolName, handler);
  }

  /**
   * Parse tool calls from AI response text
   * Extracts <tool_use> XML blocks
   */
  parseToolCalls(responseText: string): ToolCall[] {
    const toolCalls: ToolCall[] = [];

    // Match <tool_use>...</tool_use> blocks
    const toolUseRegex = /<tool_use>(.*?)<\/tool_use>/gs;
    const matches = responseText.matchAll(toolUseRegex);

    for (const match of matches) {
      const toolBlock = match[1];

      // Extract tool_name
      const toolNameMatch = toolBlock.match(/<tool_name>(.*?)<\/tool_name>/s);
      if (!toolNameMatch) continue;
      const toolName = toolNameMatch[1].trim();

      // Extract parameters
      const parameters: Record<string, string> = {};
      const parametersMatch = toolBlock.match(/<parameters>(.*?)<\/parameters>/s);
      if (parametersMatch) {
        const paramsBlock = parametersMatch[1];

        // Extract individual parameter tags
        const paramRegex = /<(\w+)>(.*?)<\/\1>/gs;
        const paramMatches = paramsBlock.matchAll(paramRegex);

        for (const paramMatch of paramMatches) {
          const paramName = paramMatch[1];
          const paramValue = paramMatch[2].trim();
          parameters[paramName] = paramValue;
        }
      }

      toolCalls.push({ toolName, parameters });
    }

    return toolCalls;
  }

  /**
   * Execute tool calls and return formatted results
   */
  async executeToolCalls(toolCalls: ToolCall[], contextId: UUID): Promise<string> {
    if (toolCalls.length === 0) return '';

    console.log(`🔧 ${this.personaName}: [TOOL] Executing ${toolCalls.length} tool(s): ${toolCalls.map(t => t.toolName).join(', ')}`);

    const results: string[] = [];

    for (const toolCall of toolCalls) {
      const startTime = Date.now();
      try {
        console.log(`🔧 ${this.personaName}: [TOOL] ${toolCall.toolName}`, toolCall.parameters);

        const handler = this.toolHandlers.get(toolCall.toolName);

        let result: ToolResult;

        if (handler) {
          // Use special handler (code/read, git commands, file/save)
          result = await handler(toolCall.parameters);
        } else if (this.toolRegistry.hasTool(toolCall.toolName)) {
          // Fall back to dynamic ToolRegistry execution
          console.log(`🔧 ${this.personaName}: [TOOL] Using dynamic handler for ${toolCall.toolName}`);
          const registryResult = await this.toolRegistry.executeTool(
            toolCall.toolName,
            toolCall.parameters,
            contextId
          );
          result = {
            toolName: registryResult.toolName,
            success: registryResult.success,
            content: registryResult.content,
            error: registryResult.error
          };
        } else {
          // Tool not found anywhere
          result = {
            toolName: toolCall.toolName,
            success: false,
            error: `Unknown tool: ${toolCall.toolName} (not in registry or special handlers)`
          };
        }

        const duration = Date.now() - startTime;

        console.log(`${result.success ? '✅' : '❌'} ${this.personaName}: [TOOL] ${toolCall.toolName} ${result.success ? 'success' : 'failed'} (${duration}ms, ${result.content?.length || 0} chars)`);

        // Log tool execution to cognition database (for interrogation)
        await CognitionLogger.logToolExecution(
          this.personaId,
          this.personaName,
          toolCall.toolName,
          toolCall.parameters,
          result.success ? 'success' : 'error',
          duration,
          'chat',  // Domain
          contextId,
          {
            toolResult: result.content?.slice(0, 1000),  // First 1000 chars of result
            errorMessage: result.error
          }
        );

        results.push(this.formatToolResult(result));
      } catch (error) {
        const errorMessage = error instanceof Error ? error.message : String(error);
        const duration = Date.now() - startTime;

        console.error(`❌ ${this.personaName}: [TOOL] ${toolCall.toolName} failed (${duration}ms):`, errorMessage);

        // Log failed tool execution
        await CognitionLogger.logToolExecution(
          this.personaId,
          this.personaName,
          toolCall.toolName,
          toolCall.parameters,
          'error',
          duration,
          'chat',
          contextId,
          {
            errorMessage
          }
        );

        results.push(this.formatToolResult({
          toolName: toolCall.toolName,
          success: false,
          error: errorMessage
        }));
      }
    }

    const successCount = results.filter(r => r.includes('<status>success</status>')).length;
    console.log(`🏁 ${this.personaName}: [TOOL] Complete: ${successCount}/${toolCalls.length} successful`);

    return results.join('\n\n');
  }

  /**
   * Format tool result as XML
   */
  private formatToolResult(result: ToolResult): string {
    if (result.success && result.content) {
      return `<tool_result>
<tool_name>${result.toolName}</tool_name>
<status>success</status>
<content>
${result.content}
</content>
</tool_result>`;
    } else {
      return `<tool_result>
<tool_name>${result.toolName}</tool_name>
<status>error</status>
<error>${result.error || 'Unknown error'}</error>
</tool_result>`;
    }
  }

  /**
   * Strip tool blocks from response text to get clean user-facing message
   */
  stripToolBlocks(responseText: string): string {
    return responseText.replace(/<tool_use>.*?<\/tool_use>/gs, '').trim();
  }

  /**
   * Get list of registered tool names
   */
  getRegisteredTools(): string[] {
    return Array.from(this.toolHandlers.keys());
  }
}
