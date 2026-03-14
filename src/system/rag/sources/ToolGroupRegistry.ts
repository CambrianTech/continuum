/**
 * ToolGroupRegistry - Semantic tool groups with intent-based selection
 *
 * Instead of dumping 30-64 tools into context, this registry organizes tools
 * into semantic groups and selects ONLY relevant groups based on the trigger
 * message content. A code question gets code tools. A chat question gets
 * communication tools. A task request gets sentinel tools.
 *
 * Each group includes:
 * - Semantic description (what these tools DO)
 * - Member tool names
 * - Intent keywords for fast matching
 * - A few-shot example showing HOW to call a tool from this group
 *
 * This solves the core problem: AIs see 290 tools, get overwhelmed, and
 * describe what they'd do instead of acting. With 5-8 contextually relevant
 * tools and a concrete example, even local models can call tools.
 */

import type { ToolDefinition } from '../../user/server/modules/ToolFormatAdapter';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('ToolGroupRegistry', 'rag');

/** A semantic group of related tools */
export interface ToolGroup {
  /** Group identifier */
  readonly id: string;
  /** Human-readable label */
  readonly label: string;
  /** What this group enables (shown to model) */
  readonly description: string;
  /** Tool name prefixes or exact names in this group */
  readonly toolPatterns: string[];
  /** Keywords that indicate this group is relevant */
  readonly intentKeywords: string[];
  /** Few-shot example of a tool call from this group (XML format) */
  readonly example: string;
  /** Always include this group regardless of intent (e.g., communication) */
  readonly alwaysInclude?: boolean;
  /** Minimum priority — groups with higher values are preferred when budget tight */
  readonly priority: number;
}

/**
 * Semantic tool groups — organized by WHAT THE AI WANTS TO DO, not by
 * command prefix. Each group is a coherent capability.
 */
const TOOL_GROUPS: readonly ToolGroup[] = [
  {
    id: 'communication',
    label: 'Communication',
    description: 'Send messages, read conversation history, reply to others',
    toolPatterns: ['collaboration/chat/send', 'collaboration/chat/export', 'collaboration/chat/history'],
    intentKeywords: ['tell', 'say', 'message', 'reply', 'ask', 'share', 'inform', 'announce', 'discuss', 'talk'],
    example: `<tool_use>
<tool_name>collaboration/chat/send</tool_name>
<parameters>{"room": "general", "message": "I found the issue — the timeout was set to 0ms instead of 60000ms."}</parameters>
</tool_use>`,
    alwaysInclude: true,
    priority: 100,
  },
  {
    id: 'code-read',
    label: 'Code Reading',
    description: 'Read files, search code, explore project structure',
    toolPatterns: ['code/read', 'code/search', 'code/tree'],
    intentKeywords: [
      'read', 'look', 'find', 'search', 'where', 'how does', 'what does',
      'show me', 'check', 'explore', 'file', 'function', 'class', 'code',
      'source', 'implementation', 'module', 'import', 'codebase', 'review',
    ],
    example: `<tool_use>
<tool_name>code/search</tool_name>
<parameters>{"pattern": "class PersonaUser", "fileGlob": "*.ts"}</parameters>
</tool_use>`,
    priority: 90,
  },
  {
    id: 'code-write',
    label: 'Code Editing',
    description: 'Edit existing files or create new ones. Always read before editing.',
    toolPatterns: ['code/edit', 'code/write', 'code/diff', 'code/verify', 'code/undo', 'code/history'],
    intentKeywords: [
      'edit', 'change', 'modify', 'update', 'fix', 'patch', 'refactor',
      'rename', 'add', 'remove', 'delete', 'create', 'write', 'implement',
      'bug', 'broken', 'wrong', 'incorrect', 'typo',
    ],
    example: `<tool_use>
<tool_name>code/edit</tool_name>
<parameters>{"filePath": "src/example.ts", "editType": "search_replace", "search": "const timeout = 0;", "replace": "const timeout = 60000;"}</parameters>
</tool_use>`,
    priority: 85,
  },
  {
    id: 'autonomous-coding',
    label: 'Autonomous Coding Agent',
    description: 'Complex multi-file tasks: build features, fix complex bugs, run pipelines',
    toolPatterns: ['sentinel/coding-agent', 'sentinel/run', 'sentinel/status'],
    intentKeywords: [
      'build', 'implement', 'create', 'feature', 'complex', 'multi-file',
      'pipeline', 'agent', 'sentinel', 'task', 'project', 'architecture',
    ],
    example: `<tool_use>
<tool_name>sentinel/coding-agent</tool_name>
<parameters>{"prompt": "Fix the memory leak in AudioResourceLifecycle — models are not being unloaded after idle timeout"}</parameters>
</tool_use>`,
    priority: 70,
  },
  {
    id: 'shell',
    label: 'Shell & Build',
    description: 'Run shell commands, build, test, deploy',
    toolPatterns: ['code/shell/execute', 'code/git'],
    intentKeywords: [
      'run', 'execute', 'build', 'test', 'deploy', 'install', 'compile',
      'shell', 'terminal', 'command', 'npm', 'cargo', 'git', 'commit',
    ],
    example: `<tool_use>
<tool_name>code/shell/execute</tool_name>
<parameters>{"cmd": "npm run build:ts", "wait": true}</parameters>
</tool_use>`,
    priority: 65,
  },
  {
    id: 'data',
    label: 'Data Access',
    description: 'Query, create, update records in collections',
    toolPatterns: ['data/'],
    intentKeywords: [
      'data', 'record', 'query', 'database', 'collection', 'entity',
      'store', 'save', 'fetch', 'list', 'count',
    ],
    example: `<tool_use>
<tool_name>data/list</tool_name>
<parameters>{"collection": "chat_messages", "filter": {"roomId": "general"}, "limit": 10}</parameters>
</tool_use>`,
    priority: 50,
  },
  {
    id: 'memory',
    label: 'Memory',
    description: 'Store and recall personal memories, knowledge',
    toolPatterns: ['memory/'],
    intentKeywords: [
      'remember', 'memory', 'recall', 'forget', 'know', 'learned',
      'noted', 'stored',
    ],
    example: `<tool_use>
<tool_name>memory/query</tool_name>
<parameters>{"query": "what do I know about the deployment process?"}</parameters>
</tool_use>`,
    priority: 45,
  },
  {
    id: 'collaboration',
    label: 'Collaboration',
    description: 'Write shared documents, propose decisions, vote',
    toolPatterns: ['collaboration/wall/', 'collaboration/decision/', 'collaboration/dm/'],
    intentKeywords: [
      'document', 'wall', 'propose', 'decision', 'vote', 'dm',
      'direct message', 'plan', 'summary', 'write up',
    ],
    example: `<tool_use>
<tool_name>collaboration/wall/write</tool_name>
<parameters>{"room": "general", "title": "Architecture Review Notes", "content": "Key findings from today's review..."}</parameters>
</tool_use>`,
    priority: 40,
  },
  {
    id: 'media',
    label: 'Media & Vision',
    description: 'Analyze images, describe screenshots, detect objects',
    toolPatterns: ['media/', 'interface/screenshot'],
    intentKeywords: [
      'image', 'screenshot', 'photo', 'picture', 'see', 'visual',
      'describe', 'detect', 'transcribe', 'audio', 'video',
    ],
    example: `<tool_use>
<tool_name>interface/screenshot</tool_name>
<parameters>{"querySelector": "chat-widget"}</parameters>
</tool_use>`,
    priority: 35,
  },
] as const;

/**
 * Registry for semantic tool groups with intent-based selection.
 */
export class ToolGroupRegistry {
  private static _instance: ToolGroupRegistry | null = null;

  static sharedInstance(): ToolGroupRegistry {
    if (!ToolGroupRegistry._instance) {
      ToolGroupRegistry._instance = new ToolGroupRegistry();
    }
    return ToolGroupRegistry._instance;
  }

  /** All registered tool groups */
  get groups(): readonly ToolGroup[] {
    return TOOL_GROUPS;
  }

  /**
   * Select relevant tool groups based on message content.
   *
   * Scores each group by keyword matches in the message, returns groups
   * that exceed a relevance threshold. Always includes 'alwaysInclude' groups.
   *
   * @param messageContent - The trigger message text
   * @param maxGroups - Maximum number of groups to return (default 4)
   * @returns Relevant tool groups sorted by relevance score
   */
  selectGroups(messageContent: string, maxGroups: number = 4): ToolGroup[] {
    if (!messageContent || messageContent.trim().length === 0) {
      // No message — return all groups sorted by priority
      return [...TOOL_GROUPS].sort((a, b) => b.priority - a.priority).slice(0, maxGroups);
    }

    const messageLower = messageContent.toLowerCase();
    const messageWords = new Set(messageLower.split(/\s+/));

    const scored: { group: ToolGroup; score: number }[] = [];

    for (const group of TOOL_GROUPS) {
      if (group.alwaysInclude) {
        scored.push({ group, score: group.priority }); // Always included at base priority
        continue;
      }

      let score = 0;
      for (const keyword of group.intentKeywords) {
        // Exact word match scores higher
        if (messageWords.has(keyword)) {
          score += 3;
        }
        // Substring match (for multi-word keywords like "how does")
        else if (messageLower.includes(keyword)) {
          score += 2;
        }
      }

      if (score > 0) {
        scored.push({ group, score });
      }
    }

    // Sort by score descending, then priority descending
    scored.sort((a, b) => {
      if (b.score !== a.score) return b.score - a.score;
      return b.group.priority - a.group.priority;
    });

    const selected = scored.slice(0, maxGroups).map(s => s.group);

    // If nothing matched (very generic message), return top-priority groups
    if (selected.length === 0) {
      return [...TOOL_GROUPS]
        .sort((a, b) => b.priority - a.priority)
        .slice(0, Math.min(3, maxGroups));
    }

    log.debug(`Selected ${selected.length} tool groups for message: ${selected.map(g => g.id).join(', ')}`);
    return selected;
  }

  /**
   * Filter tool definitions to only those matching the selected groups.
   *
   * @param tools - All available tool definitions
   * @param groups - Selected tool groups
   * @returns Filtered tools that belong to at least one selected group
   */
  filterToolsByGroups(tools: ToolDefinition[], groups: ToolGroup[]): ToolDefinition[] {
    const patterns = groups.flatMap(g => g.toolPatterns);

    return tools.filter(tool => {
      return patterns.some(pattern => {
        // Exact match or prefix match (pattern ending with /)
        if (pattern.endsWith('/')) {
          return tool.name.startsWith(pattern);
        }
        return tool.name === pattern;
      });
    });
  }

  /**
   * Build a grouped tool prompt section with examples.
   * Shows tools organized by group with a few-shot example per group.
   *
   * @param groups - Selected groups
   * @param tools - Tools filtered to these groups
   * @returns Formatted prompt section
   */
  buildGroupedPrompt(groups: ToolGroup[], tools: ToolDefinition[]): string {
    const sections: string[] = [];

    for (const group of groups) {
      const groupTools = tools.filter(tool =>
        group.toolPatterns.some(p =>
          p.endsWith('/') ? tool.name.startsWith(p) : tool.name === p
        )
      );

      if (groupTools.length === 0) continue;

      const toolNames = groupTools.map(t => t.name).join(', ');
      sections.push(
        `### ${group.label}\n${group.description}\nTools: ${toolNames}\nExample:\n${group.example}`
      );
    }

    return sections.join('\n\n');
  }
}
