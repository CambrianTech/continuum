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
import { ToolNameCodec } from './ToolFormatAdapter';

import { List } from '../../../../commands/list/shared/ListTypes';
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
  accessLevel?: ToolAccessLevel;  // Access level required to use this tool (default: 'public')
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
 * Tool Access Level - controls which personas can use which tools
 *
 * public: All personas can use (default - most tools)
 * privileged: Only trusted personas (admin-created, verified)
 * admin: Only admin personas (system owner, super-users)
 */
export type ToolAccessLevel = 'public' | 'privileged' | 'admin';

/**
 * Sensitive commands that require elevated access
 * These are commands that could cause harm if misused by untrusted personas
 */
const PRIVILEGED_COMMANDS = new Set([
  'development/exec-command',      // Arbitrary command execution
  'development/sandbox-execute',   // Sandbox execution
  'system/shutdown',               // System control
  'system/restart',                // System control
  'data/delete',                   // Data destruction
  'data/drop-collection',          // Data destruction
  'genome/fine-tune',              // Model modification
]);

const ADMIN_COMMANDS = new Set([
  'system/config/set',             // System configuration
  'user/delete',                   // User management
  'user/set-role',                 // Role assignment
  'secrets/set',                   // Secret management
  'secrets/delete',                // Secret management
  'ai/agent',                      // Prevent recursive self-invocation by personas
]);

/**
 * Determine access level required for a command
 */
function getCommandAccessLevel(commandName: string): ToolAccessLevel {
  if (ADMIN_COMMANDS.has(commandName)) return 'admin';
  if (PRIVILEGED_COMMANDS.has(commandName)) return 'privileged';
  return 'public';
}

/**
 * Filter tools based on persona access level
 */
function filterToolsByAccessLevel(
  tools: ToolDefinition[],
  personaAccessLevel: ToolAccessLevel
): ToolDefinition[] {
  const levelOrder: Record<ToolAccessLevel, number> = {
    'public': 0,
    'privileged': 1,
    'admin': 2
  };
  const personaLevel = levelOrder[personaAccessLevel];

  return tools.filter(tool => {
    const toolLevel = levelOrder[tool.accessLevel || 'public'];
    return toolLevel <= personaLevel;
  });
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

    // Query list command to discover all available commands with full metadata
    // includeDescription + includeSignature ensures we get param schemas and descriptions
    const result = await List.execute({ includeDescription: true, includeSignature: true }) as unknown as ListResult;

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

    // Register all tool names with the codec for bidirectional encoding/decoding.
    // This populates the reverse map so that any model-produced variant of a tool name
    // (e.g. code_write, $FUNCTIONS.code_write, code-write) resolves to the original.
    ToolNameCodec.instance.registerAll(toolCache);

    lastRefreshTime = Date.now();
    log(`Refreshed ${toolCache.length} tools from Commands system (codec registered)`);
  } catch (error) {
    log(`❌ Error refreshing tools: ${error}`);
  }
}

/**
 * Rich parameter descriptions for critical tools.
 * The schema generator produces generic descriptions like "filePath parameter".
 * These overrides provide meaningful descriptions so LLMs know what to pass.
 */
const PARAM_DESCRIPTION_OVERRIDES: Record<string, Record<string, string>> = {
  // ── Chat (highest priority tools) ──────────────────────────────────
  'collaboration/chat/send': {
    message: 'Text of the message to send',
    room: 'Room name to send to (e.g. "general"). Default: current room',
    replyToId: 'Short ID of message to reply to (e.g. "abc1234")',
    media: 'JSON array of media attachments [{type:"image",url:"..."}]',
  },
  'collaboration/chat/export': {
    room: 'Room name (e.g. "general")',
    limit: 'Max messages to return (default: 50)',
    afterMessageId: 'Only messages after this message ID',
    afterTimestamp: 'Only messages after this ISO timestamp',
    output: 'File path to save markdown (omit to print to stdout)',
    includeSystem: 'Include system messages (boolean)',
    includeThreading: 'Show reply-to threading (boolean)',
  },
  'collaboration/chat/poll': {
    afterMessageId: 'Message ID to poll after (returns newer messages)',
    limit: 'Max messages to return',
    room: 'Room name to poll',
  },
  'collaboration/chat/analyze': {
    roomId: 'UUID of the room to analyze',
    checkDuplicates: 'Check for duplicate messages (boolean)',
    checkTimestamps: 'Check for timestamp anomalies (boolean)',
    limit: 'Max messages to analyze',
  },

  // ── Decision/Governance ────────────────────────────────────────────
  'collaboration/decision/propose': {
    topic: 'Short title for the proposal (e.g. "Adopt TypeScript strict mode")',
    rationale: 'Why this proposal matters — your reasoning',
    options: 'JSON array of choice objects: [{"label":"Option A","description":"Details..."}]',
    description: 'Detailed description of what is being decided',
    tags: 'JSON array of tags for categorization (e.g. ["architecture","tooling"])',
    scope: 'Scope: "team", "project", or "system"',
    significanceLevel: 'Impact level: "minor", "moderate", or "major"',
  },
  'collaboration/decision/vote': {
    proposalId: 'UUID of the proposal to vote on',
    rankedChoices: 'JSON array of option IDs in preference order (best first)',
    comment: 'Optional comment explaining your vote',
  },
  'collaboration/decision/list': {
    status: 'Filter by status: "open", "closed", "all"',
    domain: 'Filter by domain/tag',
    limit: 'Max proposals to return',
  },
  'collaboration/decision/view': {
    proposalId: 'UUID of the proposal to view',
  },
  'collaboration/decision/finalize': {
    proposalId: 'UUID of the proposal to close voting on',
  },
  'collaboration/decision/create': {
    proposalId: 'UUID for the new proposal',
    topic: 'Short title for the proposal',
    rationale: 'Why this proposal matters',
    description: 'Detailed description of what is being decided',
    options: 'JSON array of choice objects with label and description',
    votingDeadline: 'ISO timestamp deadline for voting',
    requiredQuorum: 'Minimum number of votes needed',
  },

  // ── Wall (collaborative documents) ─────────────────────────────────
  'collaboration/wall/write': {
    room: 'Room name (default: current room)',
    doc: 'Document name (e.g. "meeting-notes", "architecture")',
    content: 'Markdown content to write',
    append: 'true to append to existing doc, false to overwrite',
    commitMessage: 'Description of this change (like a git commit message)',
  },
  'collaboration/wall/read': {
    room: 'Room name',
    doc: 'Document name to read',
    toc: 'true to show table of contents only',
    lines: 'Line range like "10-20"',
  },
  'collaboration/wall/list': {
    room: 'Room name',
    pattern: 'Glob pattern to filter docs (e.g. "meeting-*")',
  },
  'collaboration/wall/history': {
    room: 'Room name',
    doc: 'Document name',
    limit: 'Max history entries to show',
  },

  // ── Data ───────────────────────────────────────────────────────────
  'data/list': {
    collection: 'Collection name (e.g. "users", "chat_messages", "rooms")',
    limit: 'Max items to return',
    filter: 'JSON filter object (e.g. {"status":"active"})',
    orderBy: 'JSON array: [{"field":"createdAt","direction":"desc"}]',
    fields: 'JSON array of field names to return (projection)',
  },
  'data/get': {
    collection: 'Collection name',
    id: 'Entity UUID to retrieve',
  },

  // ── Code tools ─────────────────────────────────────────────────────
  'code/write': {
    filePath: 'Relative path to file within workspace (e.g. "index.html", "src/app.js")',
    content: 'Complete file content to write (the actual code/text, not a description)',
    description: 'Brief description of what this change does',
  },
  'code/read': {
    filePath: 'Relative path to file within workspace to read',
    startLine: 'Optional starting line number',
    endLine: 'Optional ending line number',
  },
  'code/edit': {
    filePath: 'Relative path to file within workspace to edit',
    editMode: 'Edit mode object: {editType: "search_replace", search: "old text", replace: "new text"} or {editType: "line_range", startLine: 1, endLine: 5, content: "new content"}',
    description: 'Brief description of what this edit does',
  },
  'code/tree': {
    path: 'Relative directory path within workspace (default: root ".")',
    maxDepth: 'Maximum directory depth to display',
  },
  'code/search': {
    pattern: 'Search pattern (regex supported)',
    fileGlob: 'File glob pattern to filter (e.g. "*.ts", "src/**/*.js")',
    maxResults: 'Maximum number of results to return',
  },
  'code/diff': {
    filePath: 'Relative path to file to diff',
    editType: '"search_replace", "line_range", or "insert"',
    search: 'Text to find (for search_replace)',
    replace: 'Replacement text (for search_replace)',
    startLine: 'Start line (for line_range)',
    endLine: 'End line (for line_range)',
    newContent: 'New content (for line_range)',
    line: 'Line number (for insert)',
    content: 'Content to insert (for insert)',
  },
  'code/undo': {
    changeId: 'Specific change ID to undo (from code/history)',
    count: 'Number of recent changes to undo (default: 1)',
  },
  'code/history': {
    filePath: 'File path to get history for (omit for entire workspace)',
    limit: 'Max entries to return',
  },
  'code/verify': {
    typeCheck: 'Run TypeScript type checking (boolean)',
    testFiles: 'Specific test files to run (JSON array of file paths)',
    cwd: 'Working directory override',
  },
  'code/git': {
    operation: 'Git operation: "status", "diff", "log", "add", "commit"',
    message: 'Commit message (required for "commit" operation)',
    paths: 'File paths for "add" operation (JSON array of strings)',
    staged: 'Show staged changes only (for "diff" operation)',
    count: 'Number of log entries to show (for "log" operation)',
  },
  'code/shell/execute': {
    cmd: 'Shell command to execute (e.g. "npm run build", "cargo test", "ls -la src/")',
    wait: 'Wait for completion: true = blocking (returns stdout/stderr), false = async (returns executionId). Default: false',
    timeoutMs: 'Timeout in milliseconds for blocking mode (default: 30000). Ignored in async mode',
  },
  'code/shell/watch': {
    executionId: 'Execution ID returned by code/shell/execute (async mode) to stream output from',
  },
  'code/shell/status': {
    _noParams: 'No parameters needed — returns session info for your workspace',
  },
  'code/shell/sentinel': {
    executionId: 'Execution ID to configure filter rules on',
    rules: 'JSON array of sentinel rules: [{"pattern": "error.*", "classification": "Error"}, {"pattern": "warning", "classification": "Warning"}]',
  },
  'code/shell/kill': {
    executionId: 'Execution ID of the running process to kill',
  },

  // ── System tools ───────────────────────────────────────────────────
  'ping': {
    verbose: 'Include detailed AI persona health status (boolean)',
  },
  'screenshot': {
    querySelector: 'CSS selector of element to capture (e.g. "chat-widget", "body")',
    filename: 'Output filename (default: screenshot.png)',
    fullPage: 'Capture full scrollable page (boolean)',
  },

  // ── Live collaboration ─────────────────────────────────────────────
  'collaboration/live/start': {
    participants: 'JSON array of user IDs to invite',
    name: 'Optional session name',
    withVideo: 'Enable video (boolean, default: false)',
  },
  'collaboration/dm': {
    participants: 'JSON array of user IDs for the DM room',
    name: 'Optional room display name',
  },
};

/**
 * Rich tool-level description overrides for critical tools.
 * The schema generator produces vague descriptions like "Code Write Types".
 * These overrides provide Claude Code-quality descriptions that tell the LLM
 * not just what the tool does, but HOW and WHEN to use it correctly.
 */
const TOOL_DESCRIPTION_OVERRIDES: Record<string, string> = {
  // ── Code tools (most important for coding personas) ──────────
  'code/read': 'Read file contents from workspace. Returns the file text with line numbers. You MUST read a file before editing it — editing without reading leads to wrong assumptions. Use startLine/endLine for large files.',

  'code/write': 'Create a new file or completely replace an existing file. WARNING: This overwrites the ENTIRE file. For modifying existing files, prefer code/edit (search_replace) which is surgical. Only use code/write for files that do not exist yet or when you need to replace all content.',

  'code/edit': 'Make precise edits to existing files. Supports search_replace (find exact text and replace it), line_range (replace specific lines), insert_at (add content at a line), and append (add to end). For search_replace: the search text must match EXACTLY as it appears in code/read output — character for character, including whitespace. If the search fails, re-read the file.',

  'code/search': 'Search across files in the workspace using regex patterns. Returns matching lines with file paths and line numbers. Use this instead of shell grep — it is faster and codebase-aware. Supports file glob filtering (e.g., "*.ts", "src/**/*.rs").',

  'code/tree': 'Display workspace directory structure as a tree. Use this to understand project layout before making changes. Do NOT use shell ls or find — code/tree is optimized for workspace navigation.',

  'code/diff': 'Preview an edit as a unified diff without applying it. Use this to verify your changes look correct BEFORE using code/edit. Same parameters as code/edit.',

  'code/verify': 'Run TypeScript type checking and optional tests on the workspace. Use this after EVERY edit to ensure your changes compile. If verify fails, read the error output, fix the issue, and verify again.',

  'code/undo': 'Undo recent changes. Every code/write and code/edit creates a tracked change that can be reverted. Use when an edit breaks something.',

  'code/history': 'View the change history for a file or the entire workspace. Shows what was changed, when, and the change IDs for undo.',

  'code/git': 'Git operations: status, diff, log, add, commit. Check status before committing. Use diff to review staged changes.',

  // ── Shell tools ──────────────────────────────────────────────
  'code/shell/execute': 'Execute a shell command in the workspace. Use for build commands (npm, cargo), test runners, and system operations. Do NOT use for file reading (use code/read), searching (use code/search), or directory listing (use code/tree).',

  'code/shell/watch': 'Stream output from a running async shell execution. Use after code/shell/execute with wait=false to monitor long-running processes.',

  'code/shell/status': 'Check shell session status — working directory, active executions.',

  'code/shell/kill': 'Kill a running shell execution by its execution ID.',

  // ── Chat tools ───────────────────────────────────────────────
  'collaboration/chat/send': 'Send a message to a chat room. Use the room name (e.g., "general") not a UUID.',

  'collaboration/chat/export': 'Export chat messages as markdown. Useful for reviewing conversation history.',
};

/**
 * Global parameter name → description fallback.
 * Many commands share the same parameter names (room, limit, filter, etc.).
 * This map provides decent descriptions for ALL tools without per-tool overrides.
 * Per-tool overrides in PARAM_DESCRIPTION_OVERRIDES take priority when they exist.
 */
const GLOBAL_PARAM_DESCRIPTIONS: Record<string, string> = {
  // ── Identity & targeting ─────────────────────────────────────────
  room:           'Room name (e.g. "general")',
  roomId:         'Room UUID',
  userId:         'User UUID',
  senderId:       'Sender user UUID',
  targetUserId:   'Target user UUID',
  personaId:      'Persona user UUID',
  assignee:       'Assignee user UUID',
  proposedBy:     'User UUID who proposed this',
  callerId:       'Caller user UUID',
  sessionId:      'Session UUID',
  contextId:      'Context/conversation UUID',
  activityId:     'Activity UUID',
  entityId:       'Entity UUID',
  id:             'Entity UUID',
  uniqueId:       'Unique string identifier',
  proposalId:     'Proposal UUID',
  messageId:      'Message UUID',
  changeId:       'Change/revision UUID',
  executionId:    'Execution handle UUID',

  // ── Content ──────────────────────────────────────────────────────
  message:        'Text content of the message',
  content:        'Body content (text, markdown, or code)',
  text:           'Text content',
  prompt:         'Prompt text for the AI',
  description:    'Human-readable description',
  rationale:      'Reasoning or justification',
  topic:          'Subject/title',
  comment:        'Optional comment text',
  name:           'Display name',
  displayName:    'Display name shown in UI',
  title:          'Title text',
  subtitle:       'Secondary title text',
  doc:            'Document name',
  label:          'Short label text',

  // ── Query & pagination ───────────────────────────────────────────
  limit:          'Maximum number of results',
  offset:         'Number of results to skip',
  cursor:         'Pagination cursor from previous query',
  filter:         'JSON filter object (e.g. {"status":"active"})',
  orderBy:        'Sort order: [{"field":"createdAt","direction":"desc"}]',
  orderDirection: '"asc" or "desc"',
  collection:     'Data collection name (e.g. "users", "rooms")',
  status:         'Status filter (e.g. "active", "open", "closed")',
  domain:         'Domain/category filter',
  tags:           'JSON array of tags',
  pattern:        'Search pattern (regex supported)',
  query:          'Search query string',
  fields:         'JSON array of field names to return',

  // ── File operations ──────────────────────────────────────────────
  filePath:       'File path relative to workspace',
  path:           'Directory or file path',
  startLine:      'Starting line number',
  endLine:        'Ending line number',
  lines:          'Line range (e.g. "10-20")',
  fileGlob:       'File glob pattern (e.g. "*.ts", "src/**/*.js")',
  maxDepth:       'Maximum directory depth',

  // ── Behavior flags ───────────────────────────────────────────────
  verbose:        'Include detailed output (boolean)',
  append:         'Append instead of overwrite (boolean)',
  wait:           'Wait for completion (boolean)',
  includeSystem:  'Include system messages (boolean)',
  includeMetadata:'Include metadata in output (boolean)',
  toc:            'Show table of contents only (boolean)',
  typeCheck:      'Run type checking (boolean)',

  // ── Timing ───────────────────────────────────────────────────────
  timeoutMs:      'Timeout in milliseconds',
  timestamp:      'ISO timestamp or unix milliseconds',
  afterMessageId: 'Only items after this message ID',
  afterTimestamp: 'Only items after this ISO timestamp',
  votingDeadline: 'Voting deadline (ISO timestamp)',

  // ── AI/Model ─────────────────────────────────────────────────────
  model:          'AI model identifier',
  provider:       'AI provider name (e.g. "anthropic", "openai")',
  temperature:    'Sampling temperature (0.0-1.0)',
  maxTokens:      'Maximum output tokens',
  tools:          'JSON array of tool names to enable',

  // ── Collaboration ────────────────────────────────────────────────
  replyToId:      'Message short ID to reply to',
  participants:   'JSON array of user IDs',
  role:           'Role name (e.g. "member", "admin")',
  options:        'JSON array of choice options',
  rankedChoices:  'JSON array of option IDs in preference order',
  priority:       'Priority level (0.0-1.0 or "low"/"medium"/"high")',
  scope:          'Scope: "team", "project", or "system"',
  commitMessage:  'Description of the change (like a git commit message)',
  output:         'Output file path (omit to print to stdout)',
  count:          'Number of items',
  cmd:            'Shell command to execute',
  rules:          'JSON array of rule objects',
};

/**
 * Convert CommandSignature to ToolDefinition
 */
function convertCommandToTool(cmd: CommandSignature): ToolDefinition {
  // Determine category from command name prefix
  const category = inferCategoryFromName(cmd.name);

  // Convert params to our ToolParameterSchema format
  const properties: Record<string, ParameterDefinition> = {};
  const required: string[] = [];

  // Infrastructure params that are auto-injected by the command dispatcher.
  // These must NEVER appear in tool specs — models can't provide them,
  // and APIs reject tool calls for missing required infra params.
  const INFRA_PARAMS = new Set(['userId', 'sessionId', 'contextId', 'context']);

  // Look up rich descriptions for this command (per-tool overrides take priority)
  const descOverrides = PARAM_DESCRIPTION_OVERRIDES[cmd.name];

  if (cmd.params) {
    for (const [paramName, paramInfo] of Object.entries(cmd.params)) {
      // Skip infrastructure params — auto-injected, not user-facing
      if (INFRA_PARAMS.has(paramName)) continue;

      properties[paramName] = {
        type: paramInfo.type as any,  // Trust the type from command signature
        description: descOverrides?.[paramName]
          || paramInfo.description
          || GLOBAL_PARAM_DESCRIPTIONS[paramName]
          || `${paramName}`,
        required: paramInfo.required
      };

      if (paramInfo.required) {
        required.push(paramName);
      }
    }
  }

  // Use rich description override if available, otherwise clean JSDoc artifacts
  let description: string;
  if (TOOL_DESCRIPTION_OVERRIDES[cmd.name]) {
    description = TOOL_DESCRIPTION_OVERRIDES[cmd.name];
  } else {
    // Clean JSDoc artifacts from description (schema generator captures raw comment blocks)
    // "Foo Types\n *\n * Real description" → "Real description"
    const rawDesc = cmd.description || `Execute ${cmd.name} command`;
    const cleanedDesc = rawDesc
      .replace(/^[^*]*\*\s*/gm, '')  // Strip leading " * " from JSDoc lines
      .replace(/\n\s*\n/g, '\n')     // Collapse multiple newlines
      .trim();
    // Use the last meaningful sentence if first line is just a title (e.g. "Foo Types")
    const descLines = cleanedDesc.split('\n').filter(l => l.trim().length > 0);
    description = descLines.length > 1 ? descLines.slice(1).join(' ').trim() || descLines[0] : descLines[0] || rawDesc;
  }

  return {
    name: cmd.name,
    description,
    category,
    permissions: [category + ':execute'],
    parameters: {
      type: 'object',
      properties,
      required
    },
    examples: [],  // Could add examples in future
    accessLevel: getCommandAccessLevel(cmd.name)  // Access level based on command sensitivity
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
 *
 * @param accessLevel - Optional access level to filter tools (default: 'public')
 *                      Pass persona's access level to filter out tools they can't use
 */
export async function getAllToolDefinitionsAsync(
  accessLevel?: ToolAccessLevel
): Promise<ToolDefinition[]> {
  if (toolCache.length === 0 || (Date.now() - lastRefreshTime) > CACHE_TTL_MS) {
    await refreshToolDefinitions();
  }

  // If access level specified, filter tools
  if (accessLevel) {
    return filterToolsByAccessLevel(toolCache, accessLevel);
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
  // IMPORTANT: Bias toward DOING (write, edit, execute, build) not just READING
  const essentialTools = tools.filter(t =>
    ['code/write', 'code/edit', 'code/shell/execute', 'code/verify',
     'code/read', 'code/tree', 'screenshot', 'development/build'].includes(t.name)
  );

  output += `
=== CRITICAL: DO NOT DISCUSS. DO. ===
When someone asks for code: WRITE IT. Don't explain what you would do - USE code/write.
When asked to fix something: EDIT IT. Don't describe the fix - USE code/edit.
When asked to test: RUN IT. Don't talk about testing - USE code/shell/execute.
Every response to a coding request should contain tool_use blocks, not explanations.

=== DEVELOPMENT TOOLS (read → write → build → verify) ===
`;

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
=== CODE EDITING RULES ===

1. ORIENT: code/tree to see structure, code/search to find relevant files
2. READ: ALWAYS code/read a file before editing it
3. EDIT: code/edit (search_replace) for changes, code/write ONLY for new files
4. VERIFY: code/verify after EVERY change — fix errors before moving on
5. REVIEW: code/diff to preview, code/git status before committing

CRITICAL:
- code/edit search_replace: the search text must match EXACTLY as shown in code/read output
- Prefer code/edit over code/write for existing files — code/write replaces the ENTIRE file
- Use code/search instead of shell grep, code/tree instead of shell ls
- NEVER edit a file you haven't read — your changes will be wrong
- When code/verify fails: read the errors, fix the file, verify again

Example - Edit existing file (preferred):
<tool_use>
  <tool_name>code/edit</tool_name>
  <parameters>
    <filePath>src/calculator.ts</filePath>
    <editType>search_replace</editType>
    <search>return a + b;</search>
    <replace>return a + b + 0; // ensure numeric</replace>
  </parameters>
</tool_use>

Example - Create new file:
<tool_use>
  <tool_name>code/write</tool_name>
  <parameters>
    <filePath>src/calculator.ts</filePath>
    <content>export function add(a: number, b: number): number { return a + b; }</content>
  </parameters>
</tool_use>

Example - Run build/test:
<tool_use>
  <tool_name>code/shell/execute</tool_name>
  <parameters>
    <cmd>npm run build</cmd>
  </parameters>
</tool_use>
`;

  return output;
}
