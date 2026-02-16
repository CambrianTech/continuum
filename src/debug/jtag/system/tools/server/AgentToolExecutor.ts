/**
 * AgentToolExecutor - Universal tool execution for any agentic caller
 *
 * Extracted from PersonaToolExecutor: the universal parts that ANY agent needs
 * (sentinels, personas, future autonomous agents). PersonaToolExecutor wraps
 * this with persona-specific pre/post processing (result storage, media config,
 * cognition logging, sentinel auto-config, workspace bootstrap).
 *
 * Responsibilities:
 * - Tool name correction (LLMs confuse similarly-named tools)
 * - Parameter name correction (LLMs guess wrong param names)
 * - Code/write content cleaning (CDATA, HTML entities)
 * - Room parameter resolution ("current" → actual room name)
 * - Loop detection (blocks identical calls within time window)
 * - Core execution via ToolRegistry
 * - XML tool call parsing via ToolFormatAdapters
 * - Native tool call execution with tool_use_id correlation
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { JTAGContext } from '../../core/types/JTAGTypes';
import { ToolRegistry } from './ToolRegistry';
import type { ToolExecutionResult } from './ToolRegistry';
import { RoomResolver } from '../../core/server/RoomResolver';
import { getToolFormatAdapters, unsanitizeToolName, type ToolFormatAdapter } from '../../user/server/modules/ToolFormatAdapter';
import type {
  ToolCall as NativeToolCall,
  ToolResult as NativeToolResult,
} from '../../../daemons/ai-provider-daemon/shared/AIProviderTypesV2';

// ─── Public Interfaces ───────────────────────────────────────────────

/**
 * Parsed tool call from AI response text (XML, function-style, etc.)
 */
export interface ToolCall {
  toolName: string;
  parameters: Record<string, string>;
}

/**
 * Minimal context needed for tool execution.
 * Any caller (persona, sentinel, test harness) can provide this.
 */
export interface ToolCallContext {
  /** Caller identity (persona ID, sentinel handle, test ID) */
  callerId: UUID;
  /** Session ID for command attribution */
  sessionId: UUID;
  /** Room/conversation scope */
  contextId: UUID;
  /** Optional enriched JTAG context (enables caller-adaptive output) */
  context?: JTAGContext;
}

/**
 * Result from a single tool execution
 */
export interface ToolCallResult {
  toolName: string;
  success: boolean;
  content: string;
  error?: string;
}

/**
 * Result from batch native tool execution
 */
export interface NativeToolBatchResult {
  results: NativeToolResult[];
  /** Number of tools that were blocked by loop detection */
  blockedCount: number;
}

// ─── Tool Corrections ────────────────────────────────────────────────

/**
 * Tool name corrections: LLMs sometimes confuse similarly-named tools.
 * workspace/tree shows the JTAG command hierarchy, code/tree shows workspace files.
 */
const TOOL_CORRECTIONS: Record<string, string> = {
  'workspace/tree': 'code/tree',
};

/**
 * Parameter name corrections per command prefix.
 * LLMs guess wrong parameter names when tool descriptions are generic.
 * Maps { wrongName -> correctName } for each command prefix.
 */
const PARAM_CORRECTIONS: Record<string, Record<string, string>> = {
  'code/write': {
    'path': 'filePath',
    'file': 'filePath',
    'file_path': 'filePath',
    'filepath': 'filePath',
    'filename': 'filePath',
    'file_name': 'filePath',
    'name': 'filePath',
    'contents': 'content',
    'text': 'content',
    'body': 'content',
    'data': 'content',
    'code': 'content',
    'html': 'content',
    'source': 'content',
  },
  'code/read': {
    'path': 'filePath',
    'file': 'filePath',
    'file_path': 'filePath',
    'filepath': 'filePath',
    'filename': 'filePath',
    'name': 'filePath',
    'start': 'startLine',
    'end': 'endLine',
    'from': 'startLine',
    'to': 'endLine',
  },
  'code/edit': {
    'path': 'filePath',
    'file': 'filePath',
    'file_path': 'filePath',
    'filepath': 'filePath',
    'filename': 'filePath',
    'name': 'filePath',
    'mode': 'editMode',
    'type': 'editMode',
  },
  'code/search': {
    'query': 'pattern',
    'search': 'pattern',
    'term': 'pattern',
    'regex': 'pattern',
    'glob': 'fileGlob',
    'filter': 'fileGlob',
  },
  'code/tree': {
    'directory': 'path',
    'dir': 'path',
    'folder': 'path',
    'depth': 'maxDepth',
  },
  'code/git': {
    'subcommand': 'operation',
    'command': 'operation',
    'action': 'operation',
    'op': 'operation',
    'msg': 'message',
    'files': 'paths',
  },
};

// ─── HTML Entity Decode ──────────────────────────────────────────────

const NAMED_ENTITIES: Record<string, string> = {
  lt: '<', gt: '>', amp: '&', quot: '"', apos: "'", nbsp: ' ',
};

function decodeHtmlEntities(content: string): string {
  return content.replace(
    /&(#\d+|#x[\da-fA-F]+|[a-zA-Z]+);/g,
    (match, entity: string) => {
      if (NAMED_ENTITIES[entity]) return NAMED_ENTITIES[entity];
      if (entity.startsWith('#x')) return String.fromCharCode(parseInt(entity.slice(2), 16));
      if (entity.startsWith('#')) return String.fromCharCode(parseInt(entity.slice(1), 10));
      return match;
    }
  );
}

// ─── AgentToolExecutor ───────────────────────────────────────────────

export class AgentToolExecutor {
  /**
   * Loop detection: track recent tool calls per caller to detect infinite loops.
   * Map<callerId, Array<{hash, timestamp}>>
   */
  private static readonly _recentCalls = new Map<string, Array<{ hash: string; timestamp: number }>>();
  private static readonly LOOP_WINDOW_MS = 60_000;
  private static readonly LOOP_THRESHOLD = 2;

  private readonly toolRegistry: ToolRegistry;
  private readonly formatAdapters: ToolFormatAdapter[];

  constructor() {
    this.toolRegistry = ToolRegistry.getInstance();
    this.formatAdapters = getToolFormatAdapters();
  }

  // ─── Loop Detection ──────────────────────────────────────────────

  private static hashCall(tc: ToolCall): string {
    return `${tc.toolName}:${JSON.stringify(tc.parameters)}`;
  }

  /**
   * Check if a tool call is a duplicate (appears too frequently).
   * Returns true if blocked (is a loop), false if allowed.
   */
  isLoopDetected(toolName: string, params: Record<string, string>, callerId: UUID): boolean {
    const hash = `${toolName}:${JSON.stringify(params)}`;
    const now = Date.now();

    let recent = AgentToolExecutor._recentCalls.get(callerId) ?? [];
    recent = recent.filter(e => now - e.timestamp < AgentToolExecutor.LOOP_WINDOW_MS);

    const count = recent.filter(e => e.hash === hash).length;
    recent.push({ hash, timestamp: now });
    AgentToolExecutor._recentCalls.set(callerId, recent);

    return count >= AgentToolExecutor.LOOP_THRESHOLD;
  }

  // ─── Tool Call Parsing (XML/function-style) ──────────────────────

  /**
   * Parse tool calls from AI response text using registered format adapters.
   * Supports Anthropic XML, OpenAI function-style, bare tool calls, markdown backtick.
   */
  parseToolCalls(responseText: string): ToolCall[] {
    const toolCalls: ToolCall[] = [];
    for (const adapter of this.formatAdapters) {
      const matches = adapter.matches(responseText);
      for (const match of matches) {
        const tc = adapter.parse(match);
        if (tc) toolCalls.push(tc);
      }
    }
    return toolCalls;
  }

  /**
   * Strip tool blocks from response text to get clean user-facing message.
   */
  stripToolBlocks(responseText: string): string {
    let cleaned = responseText;
    const allMatches: Array<{ start: number; end: number }> = [];

    for (const adapter of this.formatAdapters) {
      for (const match of adapter.matches(cleaned)) {
        allMatches.push({ start: match.startIndex, end: match.endIndex });
      }
    }

    allMatches.sort((a, b) => b.start - a.start);
    for (const m of allMatches) {
      cleaned = cleaned.slice(0, m.start) + cleaned.slice(m.end);
    }
    return cleaned.trim();
  }

  // ─── Name/Param Correction ───────────────────────────────────────

  /**
   * Apply tool name and parameter corrections.
   * Returns a new ToolCall (never mutates input).
   */
  correctToolCall(tc: ToolCall): { corrected: ToolCall; nameChanged: boolean; paramsChanged: string[] } {
    let toolName = tc.toolName;
    let nameChanged = false;
    const paramsChanged: string[] = [];

    // Name correction
    const correctedName = TOOL_CORRECTIONS[toolName];
    if (correctedName) {
      toolName = correctedName;
      nameChanged = true;
    }

    // Param correction
    let parameters = { ...tc.parameters };
    const corrections = PARAM_CORRECTIONS[toolName];
    if (corrections) {
      for (const [wrong, correct] of Object.entries(corrections)) {
        if (parameters[wrong] !== undefined && parameters[correct] === undefined) {
          parameters[correct] = parameters[wrong];
          delete parameters[wrong];
          paramsChanged.push(`${wrong} -> ${correct}`);
        }
      }
    }

    // Content cleaning for code/write
    if (toolName === 'code/write' && parameters.content) {
      let content = parameters.content;
      let cleaned = false;

      // Strip CDATA wrappers
      const cdataMatch = content.match(/^<!\[CDATA\[([\s\S]*?)\]\]>$/);
      if (cdataMatch) {
        content = cdataMatch[1];
        cleaned = true;
      }

      // Decode HTML entities
      const decoded = decodeHtmlEntities(content);
      if (decoded !== content) {
        content = decoded;
        cleaned = true;
      }

      if (cleaned) {
        parameters = { ...parameters, content };
      }
    }

    return {
      corrected: { toolName, parameters },
      nameChanged,
      paramsChanged,
    };
  }

  // ─── Room Resolution ─────────────────────────────────────────────

  /**
   * Resolve "current" room parameter to actual room name.
   */
  private async resolveRoomParams(
    params: Record<string, string>,
    contextId: UUID
  ): Promise<Record<string, string>> {
    if (params.room !== 'current') return params;

    const roomName = await RoomResolver.resolveCurrentParam('current', contextId);
    if (roomName) {
      return { ...params, room: roomName };
    }
    return params;
  }

  // ─── Core Execution ──────────────────────────────────────────────

  /**
   * Execute a single tool call through the full correction + execution pipeline.
   *
   * Pipeline: name correction -> param correction -> content cleaning ->
   *           room resolution -> ToolRegistry.executeTool() -> result
   */
  async executeToolCall(
    toolName: string,
    params: Record<string, string>,
    ctx: ToolCallContext
  ): Promise<ToolCallResult> {
    // Apply corrections
    const { corrected } = this.correctToolCall({ toolName, parameters: params });

    // Resolve room params
    const resolved = await this.resolveRoomParams(corrected.parameters, ctx.contextId);

    // Inject caller identity
    const paramsWithCaller = {
      ...resolved,
      userId: ctx.callerId,
      contextId: ctx.contextId,
    };

    // Execute via ToolRegistry
    const registryResult: ToolExecutionResult = await this.toolRegistry.executeTool(
      corrected.toolName,
      paramsWithCaller,
      ctx.sessionId,
      ctx.contextId,
      ctx.context
    );

    return {
      toolName: registryResult.toolName,
      success: registryResult.success,
      content: registryResult.success
        ? (registryResult.content ?? '')
        : (registryResult.error ?? 'Unknown error'),
      error: registryResult.error,
    };
  }

  /**
   * Execute native tool calls from an agentic loop.
   * Returns per-tool NativeToolResult objects with tool_use_id correlation.
   *
   * Handles loop detection: blocked calls get error results.
   * Truncates honestly if results exceed maxResultChars.
   */
  async executeNativeToolCalls(
    nativeCalls: NativeToolCall[],
    ctx: ToolCallContext,
    maxResultChars = 30_000
  ): Promise<NativeToolBatchResult> {
    if (nativeCalls.length === 0) {
      return { results: [], blockedCount: 0 };
    }

    // Convert native → internal format (decode sanitized names)
    const internalCalls: ToolCall[] = nativeCalls.map(tc => ({
      toolName: unsanitizeToolName(tc.name),
      parameters: Object.fromEntries(
        Object.entries(tc.input).map(([k, v]) => [k, String(v)])
      ) as Record<string, string>,
    }));

    // Partition: allowed vs loop-blocked
    const allowed: { idx: number; call: ToolCall }[] = [];
    let blockedCount = 0;

    for (let i = 0; i < internalCalls.length; i++) {
      const tc = internalCalls[i];
      if (this.isLoopDetected(tc.toolName, tc.parameters, ctx.callerId)) {
        blockedCount++;
      } else {
        allowed.push({ idx: i, call: tc });
      }
    }

    // Execute allowed calls in parallel
    const execResults = await Promise.all(
      allowed.map(({ call }) => this.executeToolCall(call.toolName, call.parameters, ctx))
    );

    // Map results back to native format with tool_use_id correlation
    const results: NativeToolResult[] = [];
    let execIdx = 0;

    for (let i = 0; i < nativeCalls.length; i++) {
      const tc = internalCalls[i];
      const isBlocked = this.isLoopDetected(tc.toolName, tc.parameters, ctx.callerId)
        && !allowed.some(a => a.idx === i);

      // Check if this index was in the allowed set
      const allowedEntry = allowed.find(a => a.idx === i);

      if (!allowedEntry) {
        // Blocked by loop detection
        results.push({
          toolUseId: nativeCalls[i].id,
          content: 'Tool call blocked by loop detection.',
          isError: true,
        });
        continue;
      }

      const exec = execResults[execIdx++];
      let content = exec.success ? exec.content : (exec.error ?? 'Unknown error');

      // Truncate honestly if too large
      if (content.length > maxResultChars) {
        content = content.slice(0, maxResultChars) + `\n[...truncated, ${content.length} chars total]`;
      }

      results.push({
        toolUseId: nativeCalls[i].id,
        content,
        isError: !exec.success || undefined,
      });
    }

    return { results, blockedCount };
  }

  /**
   * Execute XML-parsed tool calls and return formatted results string.
   * Used by the XML fallback path for non-native providers.
   */
  async executeXmlToolCalls(
    toolCalls: ToolCall[],
    ctx: ToolCallContext
  ): Promise<{ formattedResults: string; blockedCount: number }> {
    if (toolCalls.length === 0) {
      return { formattedResults: '', blockedCount: 0 };
    }

    // Filter loop-detected calls
    let blockedCount = 0;
    const filtered = toolCalls.filter(tc => {
      if (this.isLoopDetected(tc.toolName, tc.parameters, ctx.callerId)) {
        blockedCount++;
        return false;
      }
      return true;
    });

    if (filtered.length === 0) {
      return {
        formattedResults: '[All tool calls blocked - infinite loop detected]',
        blockedCount,
      };
    }

    // Execute all in parallel
    const results = await Promise.all(
      filtered.map(tc => this.executeToolCall(tc.toolName, tc.parameters, ctx))
    );

    // Format as XML
    const formatted = results.map(r => {
      if (r.success) {
        return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>success</status>\n<content>\n${r.content}\n</content>\n</tool_result>`;
      }
      return `<tool_result>\n<tool_name>${r.toolName}</tool_name>\n<status>error</status>\n<error>\n\`\`\`\n${r.error ?? 'Unknown error'}\n\`\`\`\n</error>\n</tool_result>`;
    }).join('\n\n');

    return { formattedResults: formatted, blockedCount };
  }

  // ─── Utility ─────────────────────────────────────────────────────

  /**
   * Get list of all available tool names from the registry.
   */
  get availableTools(): string[] {
    return this.toolRegistry.getAllTools().map(t => t.name);
  }
}
