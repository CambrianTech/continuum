/**
 * AgentSentinel - MoltBot-level autonomous coding agent
 *
 * Unlike OrchestratorSentinel which uses simple prompts, AgentSentinel:
 * 1. Has STRUCTURED TOOL DEFINITIONS - LLM sees proper tool schemas
 * 2. Uses TOOL CALLING protocol - Not regex parsing, real structured output
 * 3. Maintains FULL CONTEXT - History doesn't truncate
 * 4. ITERATES until done - Build, test, fix loop
 * 5. Can handle REAL TASKS - Multi-file changes, codebase exploration
 *
 * This is the sentinel that can build an iPhone app (or at least try).
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';
import { ModelConfig, ModelCapacity, ModelProvider, resolveModel } from './ModelProvider';
import { InferenceGenerate } from '../../commands/inference/generate/shared/InferenceGenerateTypes';
import { SentinelWorkspace, WorkspaceConfig } from './SentinelWorkspace';
import { ExecutionLogBuilder, formatExecutionLog, ExecutionLog, SentinelAction } from './SentinelExecutionLog';

// ============================================================================
// TOOL DEFINITIONS - These become the LLM's capabilities
// ============================================================================

export interface ToolDefinition {
  name: string;
  description: string;
  parameters: {
    type: 'object';
    properties: Record<string, {
      type: string;
      description: string;
      enum?: string[];
    }>;
    required: string[];
  };
}

export const AGENT_TOOLS: ToolDefinition[] = [
  {
    name: 'read_file',
    description: 'Read a file from the workspace. Returns content with line numbers.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Relative path to the file' },
        start_line: { type: 'number', description: 'Start line (1-indexed, optional)' },
        end_line: { type: 'number', description: 'End line (1-indexed, optional)' },
      },
      required: ['path'],
    },
  },
  {
    name: 'write_file',
    description: 'Write content to a file. Creates directories if needed.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Relative path to the file' },
        content: { type: 'string', description: 'File content to write' },
      },
      required: ['path', 'content'],
    },
  },
  {
    name: 'edit_file',
    description: 'Make a targeted edit to a file. Use search/replace pattern.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Relative path to the file' },
        search: { type: 'string', description: 'Exact text to find (must be unique in file)' },
        replace: { type: 'string', description: 'Text to replace it with' },
      },
      required: ['path', 'search', 'replace'],
    },
  },
  {
    name: 'search_files',
    description: 'Search for a regex pattern across all files. Returns matching lines.',
    parameters: {
      type: 'object',
      properties: {
        pattern: { type: 'string', description: 'Regex pattern to search for' },
        file_glob: { type: 'string', description: 'Glob pattern to filter files (e.g., "*.ts", "src/**/*.rs")' },
        max_results: { type: 'number', description: 'Maximum matches to return (default: 50)' },
      },
      required: ['pattern'],
    },
  },
  {
    name: 'list_files',
    description: 'List files in a directory. Shows directory tree structure.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Directory path (default: workspace root)' },
        depth: { type: 'number', description: 'Max depth to traverse (default: 3)' },
        pattern: { type: 'string', description: 'Glob pattern to filter (e.g., "*.ts")' },
      },
      required: [],
    },
  },
  {
    name: 'run_command',
    description: 'Execute a shell command. Use for builds, tests, etc.',
    parameters: {
      type: 'object',
      properties: {
        command: { type: 'string', description: 'Shell command to execute' },
        timeout_ms: { type: 'number', description: 'Timeout in milliseconds (default: 60000)' },
      },
      required: ['command'],
    },
  },
  {
    name: 'git_status',
    description: 'Show git status - modified, staged, and untracked files.',
    parameters: {
      type: 'object',
      properties: {},
      required: [],
    },
  },
  {
    name: 'git_diff',
    description: 'Show git diff of changes.',
    parameters: {
      type: 'object',
      properties: {
        path: { type: 'string', description: 'Specific file to diff (optional)' },
        staged: { type: 'boolean', description: 'Show staged changes only' },
      },
      required: [],
    },
  },
  {
    name: 'complete',
    description: 'Signal that the task is complete. Provide a summary.',
    parameters: {
      type: 'object',
      properties: {
        summary: { type: 'string', description: 'Summary of what was accomplished' },
        files_changed: { type: 'string', description: 'List of files created or modified' },
      },
      required: ['summary'],
    },
  },
  {
    name: 'give_up',
    description: 'Signal that the task cannot be completed. Explain why.',
    parameters: {
      type: 'object',
      properties: {
        reason: { type: 'string', description: 'Why the task cannot be completed' },
        attempted: { type: 'string', description: 'What was attempted' },
      },
      required: ['reason'],
    },
  },
];

// ============================================================================
// TOOL CALL TYPES
// ============================================================================

export interface ToolCall {
  name: string;
  arguments: Record<string, unknown>;
}

export interface ToolResult {
  success: boolean;
  output: string;
  error?: string;
}

// ============================================================================
// AGENT CONFIG
// ============================================================================

export interface AgentSentinelConfig {
  workingDir: string;
  maxIterations?: number;
  maxTokens?: number;
  model?: ModelConfig;
  workspace?: WorkspaceConfig;
  streamOutput?: boolean;
  onThought?: (thought: string) => void;
  onToolCall?: (tool: string, args: Record<string, unknown>) => void;
  onToolResult?: (tool: string, result: ToolResult) => void;
  onIteration?: (iteration: number, maxIterations: number) => void;
}

// ============================================================================
// MESSAGE TYPES
// ============================================================================

interface Message {
  role: 'system' | 'user' | 'assistant' | 'tool';
  content: string;
  tool_calls?: ToolCall[];
  tool_call_id?: string;
  name?: string;
}

// ============================================================================
// AGENT SENTINEL
// ============================================================================

export class AgentSentinel {
  private config: Required<Omit<AgentSentinelConfig, 'workspace' | 'model'>> & {
    workspace?: WorkspaceConfig;
    model: ModelConfig;
  };
  private workspace?: SentinelWorkspace;
  private executionLog: ExecutionLogBuilder;
  private messages: Message[] = [];
  private filesCreated: string[] = [];
  private filesModified: string[] = [];
  private currentTask: string = '';

  constructor(config: AgentSentinelConfig) {
    this.config = {
      workingDir: config.workingDir,
      maxIterations: config.maxIterations ?? 50,
      maxTokens: config.maxTokens ?? 4000,
      model: config.model ?? {
        provider: ModelProvider.LOCAL,
        capacity: ModelCapacity.MEDIUM,
        maxTokens: 4000,
      },
      workspace: config.workspace,
      streamOutput: config.streamOutput ?? true,
      onThought: config.onThought ?? ((t) => console.log(`[THINK] ${t}`)),
      onToolCall: config.onToolCall ?? ((t, a) => console.log(`[TOOL] ${t}(${JSON.stringify(a).slice(0, 100)})`)),
      onToolResult: config.onToolResult ?? ((t, r) => console.log(`[RESULT] ${t}: ${r.success ? 'OK' : 'FAIL'}`)),
      onIteration: config.onIteration ?? ((i, m) => console.log(`\n--- Iteration ${i}/${m} ---`)),
    };
    // ExecutionLogBuilder initialized in execute() when we have the task
    this.executionLog = null as unknown as ExecutionLogBuilder;
  }

  /**
   * Get the effective working directory (workspace or config)
   */
  private get workDir(): string {
    return this.workspace?.workingDir ?? this.config.workingDir;
  }

  /**
   * Execute a task
   */
  async execute(task: string): Promise<{
    success: boolean;
    summary: string;
    filesCreated: string[];
    filesModified: string[];
    iterations: number;
    executionLog: string;
  }> {
    this.currentTask = task;
    const handle = `agent-${Date.now()}`;
    this.executionLog = new ExecutionLogBuilder(handle, 'orchestrate', task);

    // Initialize workspace if configured
    if (this.config.workspace) {
      this.workspace = await SentinelWorkspace.create(this.config.workspace);
      this.executionLog.recordAction({
        type: 'file_create',
        intent: `Created ${this.config.workspace.isolation || 'branch'} workspace`,
        result: 'success',
      });
    }

    // Initialize messages with system prompt
    this.messages = [
      {
        role: 'system',
        content: this.buildSystemPrompt(),
      },
      {
        role: 'user',
        content: task,
      },
    ];

    this.config.onThought(`Task: ${task}`);

    let iteration = 0;
    let completed = false;
    let success = false;
    let summary = '';

    try {
      while (iteration < this.config.maxIterations && !completed) {
        iteration++;
        this.config.onIteration(iteration, this.config.maxIterations);

        // Get LLM response
        this.executionLog.recordAction({
          type: 'llm_query',
          intent: `Iteration ${iteration} - thinking`,
          result: 'success',
        });

        const response = await this.think();

        if (!response) {
          this.executionLog.recordAction({
            type: 'llm_query',
            intent: 'Get LLM response',
            result: 'failure',
            details: { error: 'No response from model' },
          });
          break;
        }

        // Parse tool calls from response
        const toolCalls = this.parseToolCalls(response);

        if (toolCalls.length === 0) {
          // No tool calls - LLM is just thinking, add to context and continue
          this.messages.push({ role: 'assistant', content: response });
          this.config.onThought(response.slice(0, 200));
          continue;
        }

        // Record assistant message with tool calls
        this.messages.push({
          role: 'assistant',
          content: response,
          tool_calls: toolCalls,
        });

        // Execute each tool call
        for (const toolCall of toolCalls) {
          this.config.onToolCall(toolCall.name, toolCall.arguments);

          const result = await this.executeTool(toolCall);
          this.config.onToolResult(toolCall.name, result);

          // Record tool result
          this.messages.push({
            role: 'tool',
            content: result.output,
            name: toolCall.name,
            tool_call_id: `${toolCall.name}-${iteration}`,
          });

          // Map tool name to action type
          const actionType = this.mapToolToActionType(toolCall.name);
          this.executionLog.recordAction({
            type: actionType,
            intent: `${toolCall.name}(${JSON.stringify(toolCall.arguments).slice(0, 50)})`,
            result: result.success ? 'success' : 'failure',
            details: toolCall.arguments,
            evidence: {
              output: result.output.slice(0, 1000),
              verified: result.success,
            },
          });

          // Check for completion
          if (toolCall.name === 'complete') {
            completed = true;
            success = true;
            summary = toolCall.arguments.summary as string || 'Task completed';
            break;
          }

          if (toolCall.name === 'give_up') {
            completed = true;
            success = false;
            summary = toolCall.arguments.reason as string || 'Task failed';
            break;
          }
        }
      }

      // Cleanup workspace
      if (this.workspace) {
        if (success) {
          await this.workspace.complete();
          this.executionLog.recordAction({
            type: 'file_edit',
            intent: 'Merged workspace changes',
            result: 'success',
          });
        } else {
          await this.workspace.abort();
          this.executionLog.recordAction({
            type: 'escalate',
            intent: 'Aborted workspace',
            result: 'failure',
          });
        }
      }

      if (!completed) {
        summary = `Max iterations (${this.config.maxIterations}) reached`;
      }

      const finalLog = this.executionLog.complete(success ? 'success' : 'failure');

      return {
        success,
        summary,
        filesCreated: this.filesCreated,
        filesModified: this.filesModified,
        iterations: iteration,
        executionLog: formatExecutionLog(finalLog),
      };
    } catch (error: any) {
      if (this.workspace) {
        await this.workspace.abort();
      }
      const finalLog = this.executionLog.complete('failure', { escalationReason: error.message });
      return {
        success: false,
        summary: `Error: ${error.message}`,
        filesCreated: this.filesCreated,
        filesModified: this.filesModified,
        iterations: iteration,
        executionLog: formatExecutionLog(finalLog),
      };
    }
  }

  /**
   * Map tool name to SentinelAction type
   */
  private mapToolToActionType(toolName: string): SentinelAction['type'] {
    switch (toolName) {
      case 'read_file':
      case 'list_files':
      case 'search_files':
        return 'analyze';
      case 'write_file':
        return 'file_create';
      case 'edit_file':
        return 'file_edit';
      case 'run_command':
      case 'git_status':
      case 'git_diff':
        return 'build';
      case 'complete':
      case 'give_up':
        return 'escalate';
      default:
        return 'analyze';
    }
  }

  /**
   * Build system prompt with tool definitions
   */
  private buildSystemPrompt(): string {
    const toolDescriptions = AGENT_TOOLS.map(t => {
      const params = Object.entries(t.parameters.properties)
        .map(([name, prop]) => `  - ${name}: ${prop.description}${t.parameters.required.includes(name) ? ' (required)' : ''}`)
        .join('\n');
      return `${t.name}: ${t.description}\n${params}`;
    }).join('\n\n');

    return `You are an expert software engineer working on a coding task. You have access to the following tools:

${toolDescriptions}

IMPORTANT RULES:
1. Always explore the codebase before making changes (use list_files, search_files, read_file)
2. Make targeted edits rather than rewriting entire files when possible
3. Test your changes by running builds/tests after modifications
4. If a build fails, read the error output carefully and fix the issues
5. When you're done, call 'complete' with a summary
6. If you truly cannot complete the task, call 'give_up' with an explanation

To use a tool, output a JSON block in this format:
\`\`\`tool
{"name": "tool_name", "arguments": {"arg1": "value1", "arg2": "value2"}}
\`\`\`

You can call multiple tools in one response by including multiple tool blocks.

Working directory: ${this.workDir}
`;
  }

  /**
   * Call the LLM
   */
  private async think(): Promise<string | null> {
    // Build prompt from message history
    const prompt = this.messages.map(m => {
      if (m.role === 'system') return `SYSTEM: ${m.content}`;
      if (m.role === 'user') return `USER: ${m.content}`;
      if (m.role === 'assistant') return `ASSISTANT: ${m.content}`;
      if (m.role === 'tool') return `TOOL RESULT (${m.name}): ${m.content}`;
      return '';
    }).join('\n\n');

    const result = await this.invoker.generate(prompt, this.config.model);

    if (result.success && result.text) {
      return result.text;
    }

    console.error('LLM Error:', result.error);
    return null;
  }

  /**
   * Parse tool calls from LLM response
   */
  private parseToolCalls(response: string): ToolCall[] {
    const calls: ToolCall[] = [];

    // Look for ```tool ... ``` blocks
    const toolBlockRegex = /```tool\s*\n?([\s\S]*?)```/g;
    let match;

    while ((match = toolBlockRegex.exec(response)) !== null) {
      try {
        const parsed = JSON.parse(match[1].trim());
        if (parsed.name && typeof parsed.arguments === 'object') {
          calls.push(parsed);
        }
      } catch (e) {
        // Try to fix common JSON issues
        const fixedJson = match[1].trim()
          .replace(/'/g, '"')
          .replace(/(\w+):/g, '"$1":');
        try {
          const parsed = JSON.parse(fixedJson);
          if (parsed.name && typeof parsed.arguments === 'object') {
            calls.push(parsed);
          }
        } catch {
          console.warn('Failed to parse tool call:', match[1].slice(0, 100));
        }
      }
    }

    // Also try to find inline JSON tool calls
    const inlineRegex = /\{"name":\s*"(\w+)",\s*"arguments":\s*(\{[^}]+\})\}/g;
    while ((match = inlineRegex.exec(response)) !== null) {
      try {
        const name = match[1];
        const args = JSON.parse(match[2]);
        // Avoid duplicates
        if (!calls.some(c => c.name === name && JSON.stringify(c.arguments) === JSON.stringify(args))) {
          calls.push({ name, arguments: args });
        }
      } catch {
        // Ignore parse errors
      }
    }

    return calls;
  }

  /**
   * Execute a tool call
   */
  private async executeTool(call: ToolCall): Promise<ToolResult> {
    const args = call.arguments;

    try {
      switch (call.name) {
        case 'read_file':
          return this.toolReadFile(args.path as string, args.start_line as number, args.end_line as number);

        case 'write_file':
          return this.toolWriteFile(args.path as string, args.content as string);

        case 'edit_file':
          return this.toolEditFile(args.path as string, args.search as string, args.replace as string);

        case 'search_files':
          return this.toolSearchFiles(args.pattern as string, args.file_glob as string, args.max_results as number);

        case 'list_files':
          return this.toolListFiles(args.path as string, args.depth as number, args.pattern as string);

        case 'run_command':
          return this.toolRunCommand(args.command as string, args.timeout_ms as number);

        case 'git_status':
          return this.toolGitStatus();

        case 'git_diff':
          return this.toolGitDiff(args.path as string, args.staged as boolean);

        case 'complete':
        case 'give_up':
          return { success: true, output: 'Acknowledged' };

        default:
          return { success: false, output: `Unknown tool: ${call.name}` };
      }
    } catch (error: any) {
      return { success: false, output: `Error: ${error.message}`, error: error.message };
    }
  }

  // ============================================================================
  // TOOL IMPLEMENTATIONS
  // ============================================================================

  private toolReadFile(filePath: string, startLine?: number, endLine?: number): ToolResult {
    const fullPath = path.resolve(this.workDir, filePath);

    if (!fs.existsSync(fullPath)) {
      return { success: false, output: `File not found: ${filePath}` };
    }

    const content = fs.readFileSync(fullPath, 'utf-8');
    const lines = content.split('\n');

    const start = (startLine ?? 1) - 1;
    const end = endLine ?? lines.length;
    const selectedLines = lines.slice(start, end);

    const numberedContent = selectedLines
      .map((line, i) => `${(start + i + 1).toString().padStart(4)}: ${line}`)
      .join('\n');

    return {
      success: true,
      output: `File: ${filePath} (${lines.length} lines, showing ${start + 1}-${Math.min(end, lines.length)})\n\n${numberedContent}`,
    };
  }

  private toolWriteFile(filePath: string, content: string): ToolResult {
    const fullPath = path.resolve(this.workDir, filePath);
    const dir = path.dirname(fullPath);

    if (!fs.existsSync(dir)) {
      fs.mkdirSync(dir, { recursive: true });
    }

    const existed = fs.existsSync(fullPath);
    fs.writeFileSync(fullPath, content);

    if (existed) {
      if (!this.filesModified.includes(filePath)) {
        this.filesModified.push(filePath);
      }
    } else {
      this.filesCreated.push(filePath);
    }

    return {
      success: true,
      output: `${existed ? 'Updated' : 'Created'} ${filePath} (${content.length} bytes, ${content.split('\n').length} lines)`,
    };
  }

  private toolEditFile(filePath: string, search: string, replace: string): ToolResult {
    const fullPath = path.resolve(this.workDir, filePath);

    if (!fs.existsSync(fullPath)) {
      return { success: false, output: `File not found: ${filePath}` };
    }

    const content = fs.readFileSync(fullPath, 'utf-8');

    // Check if search string exists
    const occurrences = content.split(search).length - 1;
    if (occurrences === 0) {
      return { success: false, output: `Search string not found in ${filePath}. Make sure to use exact text including whitespace.` };
    }
    if (occurrences > 1) {
      return { success: false, output: `Search string found ${occurrences} times in ${filePath}. Use a more specific search to match exactly one location.` };
    }

    const newContent = content.replace(search, replace);
    fs.writeFileSync(fullPath, newContent);

    if (!this.filesModified.includes(filePath)) {
      this.filesModified.push(filePath);
    }

    return {
      success: true,
      output: `Edited ${filePath}: replaced ${search.length} chars with ${replace.length} chars`,
    };
  }

  private toolSearchFiles(pattern: string, fileGlob?: string, maxResults?: number): ToolResult {
    const max = maxResults ?? 50;

    try {
      // Use grep for searching
      const globArg = fileGlob ? `--include="${fileGlob}"` : '';
      const cmd = `grep -rn ${globArg} -E "${pattern.replace(/"/g, '\\"')}" . 2>/dev/null | head -${max}`;

      const output = execSync(cmd, {
        cwd: this.workDir,
        encoding: 'utf-8',
        timeout: 30000,
        maxBuffer: 10 * 1024 * 1024,
      });

      const lines = output.trim().split('\n').filter(Boolean);
      return {
        success: true,
        output: `Found ${lines.length} matches:\n${output || 'No matches found'}`,
      };
    } catch (error: any) {
      if (error.status === 1) {
        return { success: true, output: 'No matches found' };
      }
      return { success: false, output: `Search error: ${error.message}` };
    }
  }

  private toolListFiles(dirPath?: string, depth?: number, pattern?: string): ToolResult {
    const targetPath = dirPath ? path.resolve(this.workDir, dirPath) : this.workDir;
    const maxDepth = depth ?? 3;

    try {
      let cmd = `find "${targetPath}" -maxdepth ${maxDepth} -type f`;
      if (pattern) {
        cmd += ` -name "${pattern}"`;
      }
      cmd += ' 2>/dev/null | head -100';

      const output = execSync(cmd, {
        cwd: this.workDir,
        encoding: 'utf-8',
        timeout: 10000,
      });

      // Make paths relative
      const relativePaths = output.trim().split('\n')
        .filter(Boolean)
        .map(p => path.relative(this.workDir, p))
        .sort();

      return {
        success: true,
        output: `Files in ${dirPath || '.'}:\n${relativePaths.join('\n') || 'No files found'}`,
      };
    } catch (error: any) {
      return { success: false, output: `List error: ${error.message}` };
    }
  }

  private toolRunCommand(command: string, timeoutMs?: number): ToolResult {
    const timeout = timeoutMs ?? 60000;

    try {
      const output = execSync(command, {
        cwd: this.workDir,
        encoding: 'utf-8',
        timeout,
        maxBuffer: 10 * 1024 * 1024,
        stdio: ['pipe', 'pipe', 'pipe'],
      });

      return {
        success: true,
        output: output.slice(0, 5000) || 'Command completed (no output)',
      };
    } catch (error: any) {
      const stderr = error.stderr?.toString() || '';
      const stdout = error.stdout?.toString() || '';
      return {
        success: false,
        output: `Exit code: ${error.status}\n\nSTDOUT:\n${stdout.slice(0, 2500)}\n\nSTDERR:\n${stderr.slice(0, 2500)}`,
        error: error.message,
      };
    }
  }

  private toolGitStatus(): ToolResult {
    try {
      const output = execSync('git status --short', {
        cwd: this.workDir,
        encoding: 'utf-8',
        timeout: 10000,
      });

      return {
        success: true,
        output: output || 'Working tree clean',
      };
    } catch (error: any) {
      return { success: false, output: `Git error: ${error.message}` };
    }
  }

  private toolGitDiff(filePath?: string, staged?: boolean): ToolResult {
    try {
      let cmd = 'git diff';
      if (staged) cmd += ' --staged';
      if (filePath) cmd += ` -- "${filePath}"`;

      const output = execSync(cmd, {
        cwd: this.workDir,
        encoding: 'utf-8',
        timeout: 10000,
        maxBuffer: 10 * 1024 * 1024,
      });

      return {
        success: true,
        output: output.slice(0, 5000) || 'No changes',
      };
    } catch (error: any) {
      return { success: false, output: `Git error: ${error.message}` };
    }
  }
}

