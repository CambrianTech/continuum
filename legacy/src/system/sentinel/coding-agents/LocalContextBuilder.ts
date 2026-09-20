/**
 * LocalContextBuilder — Dynamic budget-aware system prompt for local inference.
 *
 * Builds system prompts that fit within the tight token budget of GGUF local
 * models (350 tokens) or BF16 safetensors (800 tokens).
 *
 * Format is the EXACT QAT training format (verified from diagnose_prefill.rs):
 *   header → tool lines → footer
 *   "- toolname: Short description. Params: {param: type}"
 *
 * Tool descriptions MUST be very short (~3-5 words) to match training distribution.
 * Out-of-distribution additions (workspace context, "Working directory:", verbose
 * descriptions) cause hallucination and repetition loops. The TRAINING_DESCRIPTIONS
 * table provides the canonical short-form descriptions for core tools.
 *
 * Tool selection is task-aware: "list files" → shell tools;
 * "refactor this file" → code tools. No hardcoded tool list.
 */

import { ToolGroupRegistry } from '@system/rag/sources/ToolGroupRegistry';
import {
  getAllToolDefinitionsAsync,
  type ToolDefinition,
  type ToolAccessLevel,
} from '@system/user/server/modules/PersonaToolDefinitions';

/**
 * Training-time short descriptions for core tools.
 *
 * These MUST match the QAT training format exactly (from diagnose_prefill.rs line 42).
 * Any tool not in this table falls back to first-word-group truncation.
 * Descriptions are ~3-5 words to stay within per-tool token budget.
 */
const TRAINING_DESCRIPTIONS: Record<string, string> = {
  'code/write': 'Create a NEW file',
  'code/read': 'Read an existing file',
  'code/edit': 'Modify an existing file',
  'code/shell/execute': 'Run a shell command',
  'code/tree': 'List directory structure',
  'code/search': 'Search for text in files',
  'code/diff': 'Preview an edit as a diff',
  'code/undo': 'Undo the last file change',
  'code/shell/status': 'Check shell session status',
  'code/shell/watch': 'Stream output from a shell run',
};

/**
 * Training-time param overrides for core tools.
 *
 * Matches the EXACT params from diagnose_prefill.rs.
 * Prevents verbose PersonaToolDefinitions param descriptions from leaking in.
 */
const TRAINING_PARAMS: Record<string, string> = {
  'code/write': 'filePath: string, content: string',
  'code/read': 'filePath: string',
  'code/edit': 'filePath: string, oldString: string, newString: string',
  'code/shell/execute': 'command: string',
  'code/tree': 'path: string',
  'code/search': 'query: string, path: string',
};

export interface LocalContextSpec {
  /** Working directory for the agent */
  cwd: string;
  /** Task prompt — drives intent-based tool group selection */
  taskPrompt: string;
  /** Hard token cap for the system prompt (350 for GGUF, 800 for BF16) */
  maxSystemTokens: number;
  /** Optional extra instructions appended at the end */
  customSystemPrompt?: string;
}

export interface LocalContextResult {
  systemPrompt: string;
  estimatedTokens: number;
  /** Tool names included in the prompt (for caller reference; tools=[] passed to ai/agent) */
  selectedTools: string[];
}

export class LocalContextBuilder {
  private static _instance: LocalContextBuilder;

  static sharedInstance(): LocalContextBuilder {
    if (!LocalContextBuilder._instance) {
      LocalContextBuilder._instance = new LocalContextBuilder();
    }
    return LocalContextBuilder._instance;
  }

  async build(spec: LocalContextSpec): Promise<LocalContextResult> {
    // ALWAYS include training-set tools first — these are the tools the model
    // was trained on (QAT v2). Out-of-distribution tools cause hallucination.
    // Additional tools from ToolGroupRegistry fill remaining budget.
    const trainingToolNames = Object.keys(TRAINING_DESCRIPTIONS);

    const maxTools = spec.maxSystemTokens <= 400 ? 6 : 10;
    const maxToolChars = Math.floor(spec.maxSystemTokens * 0.45) * 4;

    // Phase 1: Add training-set tools (always included, model knows these)
    const toolLines: string[] = [];
    let toolCharsUsed = 0;
    for (const name of trainingToolNames) {
      if (toolLines.length >= maxTools) break;
      const desc = TRAINING_DESCRIPTIONS[name];
      const params = TRAINING_PARAMS[name];
      const line = params
        ? `- ${name}: ${desc}. Params: {${params}}`
        : `- ${name}: ${desc}`;
      if (toolCharsUsed + line.length + 1 > maxToolChars) break;
      toolLines.push(line);
      toolCharsUsed += line.length + 1;
    }

    // Phase 2: Fill remaining budget with task-relevant tools from registry
    const groupRegistry = ToolGroupRegistry.sharedInstance();
    const selectedGroups = groupRegistry.selectGroups(spec.taskPrompt, 3);
    const toolPatterns = selectedGroups.flatMap(g => g.toolPatterns);
    const includedNames = new Set(trainingToolNames);

    let defs: ToolDefinition[] = [];
    try {
      const allDefs = await getAllToolDefinitionsAsync('public' as ToolAccessLevel);
      defs = allDefs.filter(t =>
        !includedNames.has(t.name) &&
        toolPatterns.some(p =>
          p.endsWith('/') ? t.name.startsWith(p) : t.name === p,
        ),
      );
    } catch {
      // Tool definitions unavailable — continue with training tools only
    }

    for (const t of defs) {
      if (toolLines.length >= maxTools) break;
      const line = this.formatToolLine(t);
      if (toolCharsUsed + line.length + 1 > maxToolChars) break;
      toolLines.push(line);
      toolCharsUsed += line.length + 1;
    }

    // Training-time system prompt format (from diagnose_prefill.rs).
    // Any deviation from this structure causes model hallucination.
    const parts: string[] = [
      'You are a coding agent working in a project directory. You have these tools:',
      '',
      toolLines.join('\n'),
      '',
      'Use <tool_use> XML format to call tools. Always use code/write for NEW files, code/edit for MODIFYING existing files, code/read before editing.',
    ];

    if (spec.customSystemPrompt) {
      parts.push('', spec.customSystemPrompt);
    }

    const systemPrompt = parts.join('\n');


    return {
      systemPrompt,
      estimatedTokens: Math.ceil(systemPrompt.length / 4),
      selectedTools: toolLines.map(l => {
        const colonIdx = l.indexOf(':');
        return colonIdx > 2 ? l.slice(2, colonIdx) : l.slice(2);
      }),
    };
  }

  /**
   * Format a tool as a training-format one-liner.
   *
   * Uses TRAINING_DESCRIPTIONS for known tools (matches QAT training exactly).
   * Falls back to first-word-group of the full description for unknown tools.
   * Param list uses TRAINING_PARAMS overrides for known tools.
   */
  private formatToolLine(t: ToolDefinition): string {
    // Training description: exact match from QAT training data.
    const desc = TRAINING_DESCRIPTIONS[t.name] ?? this.shortDescription(t.description);

    // Training params: use override if available, else derive from schema.
    const paramStr = TRAINING_PARAMS[t.name] ?? this.deriveParams(t);

    return paramStr
      ? `- ${t.name}: ${desc}. Params: {${paramStr}}`
      : `- ${t.name}: ${desc}`;
  }

  /** Extract a short description: up to the first comma/semicolon or 25 chars, at word boundary. */
  private shortDescription(desc: string): string {
    const firstPart = desc.split(/[.,;]/)[0];
    if (firstPart.length <= 25) return firstPart;
    const truncated = firstPart.slice(0, 25);
    const lastSpace = truncated.lastIndexOf(' ');
    return lastSpace > 5 ? truncated.slice(0, lastSpace) : truncated;
  }

  /** Derive required-params string from tool definition schema. */
  private deriveParams(t: ToolDefinition): string {
    const props = t.parameters?.properties ?? {};
    const required = new Set<string>(t.parameters?.required ?? []);
    return Object.entries(props)
      .filter(([k]) => required.has(k))
      .map(([k, v]) => `${k}: ${v.type}`)
      .join(', ');
  }
}
