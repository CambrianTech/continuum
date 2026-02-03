/**
 * ToolRegistry - Dynamic tool discovery from Commands
 *
 * Replaces hardcoded tool lists with dynamic command discovery.
 * Tools are auto-generated from CommandSignature metadata via 'list' command.
 *
 * Architecture:
 * 1. Query 'list' command on startup → get all CommandSignatures
 * 2. Auto-generate tool handlers for ANY command (universal wrapper)
 * 3. Auto-generate tool descriptions for AI system prompts
 *
 * Benefits:
 * - New commands (decision/propose, decision/rank, etc.) automatically become tools
 * - Zero hardcoding - single source of truth is CommandRegistry
 * - Consistent tool behavior across all commands
 */

import { Commands } from '../../core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { CommandSignature } from '../../../commands/list/shared/ListTypes';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { MediaItem } from '../../data/entities/ChatMessageEntity';
import type { CommandParams, CommandResult } from '../../core/types/JTAGTypes';
import { AIProviderDaemon } from '../../../daemons/ai-provider-daemon/shared/AIProviderDaemon';
import { getSearchWorkerClient } from '../../../shared/ipc/SearchWorkerClient';

import { List } from '../../../commands/list/shared/ListTypes';
/**
 * Type guard for command results that include a success field
 * Many commands add success: boolean to their result type
 */
interface ResultWithSuccess {
  success: boolean;
  error?: string;
}

function hasSuccessField(result: CommandResult): result is CommandResult & ResultWithSuccess {
  return 'success' in result && typeof (result as { success?: unknown }).success === 'boolean';
}

/**
 * Type guard for command results that include media
 * Some commands (screenshot, file/read, etc.) include media in their results
 */
interface ResultWithMedia {
  media?: MediaItem | MediaItem[];
}

function hasMediaField(result: CommandResult): result is CommandResult & ResultWithMedia {
  return 'media' in result;
}

/**
 * Tool metadata for AI consumption
 */
export interface ToolDefinition {
  name: string;
  description: string;
  category: string;
  parameters: Record<string, {
    type: string;
    required: boolean;
    description?: string;
  }>;
  examples?: string[];
}

/**
 * Tool execution result with structured media support
 */
export interface ToolExecutionResult {
  toolName: string;
  success: boolean;
  content?: string;
  media?: MediaItem[];  // Structured media from command results
  error?: string;
}

/**
 * ToolRegistry - Dynamic command discovery and tool generation
 */
export class ToolRegistry {
  private static instance: ToolRegistry;
  private tools: Map<string, ToolDefinition> = new Map();
  private initialized = false;

  // Semantic search: tool embeddings cache
  private toolEmbeddings: Map<string, number[]> = new Map();
  private embeddingsGeneratedAt: number = 0;
  private readonly EMBEDDINGS_TTL_MS = 5 * 60 * 1000; // 5 min (matches tool cache)
  private embeddingsGenerating: Promise<void> | null = null; // Prevent concurrent generation

  private constructor() {}

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
   * Initialize tool registry by querying 'list' command
   */
  async initialize(): Promise<void> {
    if (this.initialized) {
      console.log('⚙️ ToolRegistry: Already initialized');
      return;
    }

    console.log('⚙️ ToolRegistry: Discovering available commands...');

    try {
      const result = await List.execute({ includeDescription: true }) as unknown as {
        commands?: CommandSignature[];
        success: boolean;
        error?: string;
      };

      if (!result.success || !result.commands) {
        throw new Error(`Failed to list commands: ${result.error}`);
      }

      // Convert CommandSignatures to ToolDefinitions
      for (const cmd of result.commands) {
        const toolDef: ToolDefinition = {
          name: cmd.name,
          description: cmd.description,
          category: 'command',  // All commands are category 'command'
          parameters: cmd.params || {}
        };
        this.tools.set(cmd.name, toolDef);
      }

      // Register built-in meta-tools for tool discovery
      this.registerBuiltInTools();

      this.initialized = true;
      console.log(`✅ ToolRegistry: Discovered ${this.tools.size} commands as tools (including ${ToolRegistry.BUILT_IN_TOOLS.length} meta-tools)`);
    } catch (error) {
      console.error(`❌ ToolRegistry: Initialization failed:`, error);
      throw error;
    }
  }

  /**
   * Get all available tools
   */
  getAllTools(): ToolDefinition[] {
    if (!this.initialized) {
      throw new Error('ToolRegistry not initialized - call initialize() first');
    }
    return Array.from(this.tools.values());
  }

  /**
   * Get specific tool by name
   */
  getTool(name: string): ToolDefinition | undefined {
    return this.tools.get(name);
  }

  /**
   * Check if tool exists
   */
  hasTool(name: string): boolean {
    return this.tools.has(name);
  }

  // ===========================================================================
  // TOOL DISCOVERY - For personas to find tools without loading all into context
  // ===========================================================================

  /**
   * Clean and truncate a description for tool listings
   * Strips JSDoc comment formatting and limits to first sentence or 100 chars
   */
  private cleanDescription(desc: string | undefined, maxLength: number = 100): string {
    if (!desc) return '';

    // Remove JSDoc comment prefixes (e.g., " * ", "* ")
    let cleaned = desc.replace(/^\s*\*\s*/gm, '').replace(/\n\s*\*\s*/g, ' ');

    // Remove section headers (e.g., "====" lines)
    cleaned = cleaned.replace(/={3,}/g, '').trim();

    // Get first sentence (up to first period, exclamation, or newline)
    const firstSentence = cleaned.split(/[.\n!]/)[0];
    if (firstSentence && firstSentence.length > 10) {
      cleaned = firstSentence;
    }

    // Collapse whitespace
    cleaned = cleaned.replace(/\s+/g, ' ').trim();

    // Truncate if still too long
    if (cleaned.length > maxLength) {
      return cleaned.slice(0, maxLength - 3) + '...';
    }

    return cleaned;
  }

  /**
   * Search tools by keyword (matches name and description)
   * Same algorithm as MCP search_tools for consistency
   *
   * IMPORTANT: If category filter yields 0 results, falls back to searching ALL tools.
   */
  searchTools(query: string, category?: string, limit: number = 10): Array<{ name: string; description: string; category: string }> {
    const queryLower = query.toLowerCase();

    // Helper to search a set of tools
    const searchToolSet = (toolSet: Iterable<ToolDefinition>): Array<{ name: string; description: string; category: string; score: number }> => {
      const results: Array<{ name: string; description: string; category: string; score: number }> = [];

      for (const tool of toolSet) {
        const nameLower = tool.name.toLowerCase();
        const descLower = (tool.description || '').toLowerCase();

        // Score matches
        let score = 0;
        if (nameLower.includes(queryLower)) score += 10;
        if (nameLower.startsWith(queryLower)) score += 5;
        if (descLower.includes(queryLower)) score += 3;

        // Exact segment match (e.g., "css" matches "widget-css")
        const segments = nameLower.split(/[\/\-_]/);
        if (segments.includes(queryLower)) score += 8;

        if (score > 0) {
          const toolCategory = nameLower.includes('/') ? nameLower.split('/')[0] : 'root';
          results.push({
            name: tool.name,
            description: this.cleanDescription(tool.description, 120) || tool.name,
            category: toolCategory,
            score,
          });
        }
      }

      return results;
    };

    // First, try with category filter if provided
    if (category) {
      const categoryPrefix = category.endsWith('/') ? category : `${category}/`;
      const filteredTools = Array.from(this.tools.values()).filter(t => {
        const nameLower = t.name.toLowerCase();
        return nameLower.startsWith(categoryPrefix) || nameLower === category;
      });

      const categoryResults = searchToolSet(filteredTools);
      if (categoryResults.length > 0) {
        categoryResults.sort((a, b) => b.score - a.score || a.name.localeCompare(b.name));
        return categoryResults.slice(0, limit).map(({ name, description, category }) => ({ name, description, category }));
      }
      // Fall through to search ALL tools if category had no results
    }

    // Search all tools
    const results = searchToolSet(this.tools.values());
    results.sort((a, b) => b.score - a.score || a.name.localeCompare(b.name));
    return results.slice(0, limit).map(({ name, description, category }) => ({ name, description, category }));
  }

  /**
   * BM25 search for tools (better than keyword, runs on Rust worker off main thread)
   * Returns tools ranked by BM25 score with TF-IDF weighting
   *
   * IMPORTANT: If category filter yields 0 results, falls back to searching ALL tools.
   * This ensures users can find tools like "ai/rag/inspect" even if they guess wrong category.
   */
  async bm25SearchTools(
    query: string,
    category?: string,
    limit: number = 10
  ): Promise<Array<{ name: string; description: string; category: string; score: number }>> {
    const client = getSearchWorkerClient();

    // Check if worker is available
    if (!client.isAvailable()) {
      console.log('⚠️ ToolRegistry: Search worker not available, falling back to keyword search');
      return this.searchTools(query, category, limit).map(t => ({ ...t, score: 1.0 }));
    }

    // Build corpus from tools
    const allTools = this.getAllTools();

    // Helper to run BM25 search on a tool set
    const runSearch = async (toolSet: ToolDefinition[]): Promise<Array<{ name: string; description: string; category: string; score: number }>> => {
      if (toolSet.length === 0) return [];

      const corpus = toolSet.map(t => `${t.name}: ${t.description}`);
      const results = await client.bm25(query, corpus);

      return results
        .filter(r => r.score > 0.01)
        .slice(0, limit)
        .map(r => {
          const tool = toolSet[r.index];
          const toolCategory = tool.name.includes('/') ? tool.name.split('/')[0] : 'root';
          return {
            name: tool.name,
            description: this.cleanDescription(tool.description, 120) || tool.name,
            category: toolCategory,
            score: Math.round(r.score * 1000) / 1000,
          };
        });
    };

    try {
      // First, try with category filter if provided
      if (category) {
        const categoryPrefix = category.endsWith('/') ? category : `${category}/`;
        const filteredTools = allTools.filter(t => {
          const nameLower = t.name.toLowerCase();
          return nameLower.startsWith(categoryPrefix) || nameLower === category;
        });

        const categoryResults = await runSearch(filteredTools);

        // If category search found results, return them
        if (categoryResults.length > 0) {
          return categoryResults;
        }

        // Category filter yielded 0 results - fall back to searching ALL tools
        // This is the key fix: user searched for "inspect" in "development" category,
        // but it's actually in "ai/rag/inspect" - we should still find it!
        console.log(`⚠️ ToolRegistry: No results in category "${category}", searching all tools...`);
      }

      // Search all tools (either no category provided, or category had 0 results)
      return await runSearch(allTools);
    } catch (error) {
      console.error('❌ ToolRegistry: BM25 search failed:', error);
      // Fallback to keyword search
      return this.searchTools(query, category, limit).map(t => ({ ...t, score: 1.0 }));
    }
  }

  /**
   * List tools by category (for browsing all available tools)
   */
  listToolsByCategory(category?: string, limit: number = 50): Array<{ name: string; description: string; category: string }> {
    const results: Array<{ name: string; description: string; category: string }> = [];

    for (const tool of this.tools.values()) {
      const nameLower = tool.name.toLowerCase();
      const toolCategory = nameLower.includes('/') ? nameLower.split('/')[0] : 'root';

      // Category filter
      if (category) {
        const categoryPrefix = category.endsWith('/') ? category : `${category}/`;
        if (!nameLower.startsWith(categoryPrefix) && toolCategory !== category) {
          continue;
        }
      }

      results.push({
        name: tool.name,
        description: this.cleanDescription(tool.description, 120) || tool.name,
        category: toolCategory,
      });
    }

    // Sort by category then name
    results.sort((a, b) => a.category.localeCompare(b.category) || a.name.localeCompare(b.name));
    return results.slice(0, limit);
  }

  /**
   * Get available categories
   */
  getCategories(): string[] {
    const categories = new Set<string>();
    for (const tool of this.tools.values()) {
      const name = tool.name.toLowerCase();
      const category = name.includes('/') ? name.split('/')[0] : 'root';
      categories.add(category);
    }
    return Array.from(categories).sort();
  }

  // ===========================================================================
  // SEMANTIC SEARCH - Find tools by meaning, not just keywords
  // ===========================================================================

  /**
   * Ensure tool embeddings are cached (lazy generation with TTL)
   */
  private async ensureToolEmbeddings(): Promise<void> {
    const now = Date.now();
    const isFresh = this.toolEmbeddings.size > 0 &&
                    (now - this.embeddingsGeneratedAt) < this.EMBEDDINGS_TTL_MS;

    if (isFresh) return;

    // If already generating, wait for that to complete
    if (this.embeddingsGenerating) {
      await this.embeddingsGenerating;
      return;
    }

    // Generate embeddings for all tools
    this.embeddingsGenerating = this.generateToolEmbeddings();
    try {
      await this.embeddingsGenerating;
    } finally {
      this.embeddingsGenerating = null;
    }
  }

  /**
   * Generate embeddings for all tools
   */
  private async generateToolEmbeddings(): Promise<void> {
    const tools = this.getAllTools();
    const texts = tools.map(t => `${t.name}: ${t.description}`);

    console.log(`🔍 ToolRegistry: Generating embeddings for ${tools.length} tools...`);
    const startTime = Date.now();

    try {
      const response = await AIProviderDaemon.createEmbedding({
        input: texts,
        model: 'nomic-embed-text', // Local Ollama, fast
      });

      // Cache results
      this.toolEmbeddings.clear();
      tools.forEach((tool, i) => {
        if (response.embeddings[i]) {
          this.toolEmbeddings.set(tool.name, response.embeddings[i]);
        }
      });
      this.embeddingsGeneratedAt = Date.now();

      const elapsed = Date.now() - startTime;
      console.log(`✅ ToolRegistry: Generated ${this.toolEmbeddings.size} embeddings in ${elapsed}ms`);
    } catch (error) {
      console.error('❌ ToolRegistry: Failed to generate embeddings:', error);
      throw error;
    }
  }

  /**
   * Semantic search for tools by meaning
   * Returns tools ranked by cosine similarity to query
   */
  async semanticSearchTools(
    query: string,
    limit: number = 10
  ): Promise<Array<{ name: string; description: string; category: string; similarity: number }>> {
    await this.ensureToolEmbeddings();

    // Embed the query
    const queryResponse = await AIProviderDaemon.createEmbedding({
      input: [query],
      model: 'nomic-embed-text',
    });
    const queryVector = queryResponse.embeddings[0];

    if (!queryVector) {
      throw new Error('Failed to generate query embedding');
    }

    // Compute similarities
    const results: Array<{ name: string; description: string; category: string; similarity: number }> = [];

    for (const tool of this.tools.values()) {
      const toolVector = this.toolEmbeddings.get(tool.name);
      if (!toolVector) continue;

      const similarity = this.cosineSimilarity(queryVector, toolVector);
      if (similarity > 0.3) { // Threshold for relevance
        const category = tool.name.includes('/') ? tool.name.split('/')[0] : 'root';
        results.push({
          name: tool.name,
          description: this.cleanDescription(tool.description, 120) || tool.name,
          category,
          similarity: Math.round(similarity * 1000) / 1000, // Round to 3 decimals
        });
      }
    }

    // Sort by similarity descending
    return results
      .sort((a, b) => b.similarity - a.similarity)
      .slice(0, limit);
  }

  /**
   * Cosine similarity between two vectors
   */
  private cosineSimilarity(a: number[], b: number[]): number {
    if (a.length !== b.length) return 0;

    let dot = 0, magA = 0, magB = 0;
    for (let i = 0; i < a.length; i++) {
      dot += a[i] * b[i];
      magA += a[i] * a[i];
      magB += b[i] * b[i];
    }
    const magnitude = Math.sqrt(magA) * Math.sqrt(magB);
    return magnitude === 0 ? 0 : dot / magnitude;
  }

  // ===========================================================================
  // BUILT-IN META-TOOLS - Tools for discovering other tools
  // ===========================================================================

  private static readonly BUILT_IN_TOOLS: ToolDefinition[] = [
    {
      name: 'search_tools',
      description: 'Search for tools by keyword. Use this to find specialized tools without knowing exact names.',
      category: 'meta',
      parameters: {
        query: { type: 'string', required: true, description: 'Search keyword (e.g., "screenshot", "css", "chat")' },
        category: { type: 'string', required: false, description: 'Optional category filter (e.g., "interface", "ai", "data")' },
        limit: { type: 'number', required: false, description: 'Max results (default: 10)' },
      },
    },
    {
      name: 'list_tools',
      description: 'List available tools, optionally filtered by category. Use this to browse all capabilities.',
      category: 'meta',
      parameters: {
        category: { type: 'string', required: false, description: 'Filter by category (e.g., "interface", "collaboration", "ai")' },
        limit: { type: 'number', required: false, description: 'Max results (default: 50)' },
      },
    },
    {
      name: 'list_categories',
      description: 'List all available tool categories.',
      category: 'meta',
      parameters: {},
    },
  ];

  /**
   * Register built-in meta-tools
   */
  private registerBuiltInTools(): void {
    for (const tool of ToolRegistry.BUILT_IN_TOOLS) {
      this.tools.set(tool.name, tool);
    }
  }

  /**
   * Check if tool is a built-in meta-tool
   */
  private isBuiltInTool(name: string): boolean {
    return ToolRegistry.BUILT_IN_TOOLS.some(t => t.name === name);
  }

  /**
   * Execute built-in meta-tool (async for semantic search)
   */
  private async executeBuiltInTool(toolName: string, parameters: Record<string, string>): Promise<ToolExecutionResult> {
    switch (toolName) {
      case 'search_tools': {
        const query = parameters.query || '';
        const category = parameters.category;
        const limit = parameters.limit ? parseInt(parameters.limit, 10) : 10;

        // BM25 search (better than keyword, runs on Rust worker off main thread)
        let results = await this.bm25SearchTools(query, category, limit);
        let searchType = 'bm25';

        // If few results and query is meaningful, try semantic search
        if (results.length < 3 && query.length > 2) {
          try {
            const semanticResults = await this.semanticSearchTools(query, limit);
            searchType = 'hybrid';

            // Merge results: BM25 first, then semantic (dedupe)
            const seen = new Set(results.map(r => r.name));
            for (const sr of semanticResults) {
              if (!seen.has(sr.name)) {
                // Category filter for semantic results too
                if (category) {
                  const categoryPrefix = category.endsWith('/') ? category : `${category}/`;
                  if (!sr.name.toLowerCase().startsWith(categoryPrefix) && sr.category !== category) {
                    continue;
                  }
                }
                results.push({ name: sr.name, description: sr.description, category: sr.category, score: sr.similarity });
                seen.add(sr.name);
              }
            }
            results = results.slice(0, limit);
          } catch (err) {
            console.log('⚠️ Semantic search fallback failed:', err);
            // Keep BM25 results, searchType stays 'bm25'
          }
        }

        return {
          toolName,
          success: true,
          content: JSON.stringify({
            query,
            category: category || 'all',
            count: results.length,
            tools: results,
            searchType,
          }, null, 2),
        };
      }
      case 'list_tools': {
        const category = parameters.category;
        const limit = parameters.limit ? parseInt(parameters.limit, 10) : 50;
        const results = this.listToolsByCategory(category, limit);
        return {
          toolName,
          success: true,
          content: JSON.stringify({
            category: category || 'all',
            count: results.length,
            tools: results,
          }, null, 2),
        };
      }
      case 'list_categories': {
        const categories = this.getCategories();
        return {
          toolName,
          success: true,
          content: JSON.stringify({
            count: categories.length,
            categories,
          }, null, 2),
        };
      }
      default:
        return {
          toolName,
          success: false,
          error: `Unknown built-in tool: ${toolName}`,
        };
    }
  }

  /**
   * Execute a tool (universal command wrapper)
   *
   * This is the "adapter" the user mentioned - ONE function that can execute ANY command
   */
  async executeTool(
    toolName: string,
    parameters: Record<string, string>,
    sessionId: UUID,  // SessionId of the tool executor (AI's own session for sandboxing)
    contextId: UUID,
    context?: import('../../core/types/JTAGTypes').JTAGContext  // Optional enriched context (with callerType for caller-adaptive output)
  ): Promise<ToolExecutionResult> {
    // Handle built-in meta-tools first (no command execution needed)
    if (this.isBuiltInTool(toolName)) {
      return await this.executeBuiltInTool(toolName, parameters);
    }

    if (!this.hasTool(toolName)) {
      return {
        toolName,
        success: false,
        error: `Unknown tool: ${toolName}. Use search_tools to find available tools.`
      };
    }

    // NO try-catch - let exceptions bubble to PersonaResponseGenerator
    // Commands.execute (via CommandDaemon) returns {success: false, error} for expected command failures
    // Only UNEXPECTED exceptions (system crashes) should bubble up as exceptions

    // Parse JSON parameters if needed
    const parsedParams: Record<string, any> = {};
    const toolDef = this.getTool(toolName)!;

    for (const [key, value] of Object.entries(parameters)) {
      const paramDef = toolDef.parameters[key];

      // Try to parse JSON for complex types
      if (paramDef && (paramDef.type === 'object' || paramDef.type === 'array')) {
        try {
          parsedParams[key] = JSON.parse(value);
        } catch {
          parsedParams[key] = value; // Fallback to string
        }
      } else if (paramDef && paramDef.type === 'number') {
        parsedParams[key] = parseInt(value, 10);
      } else if (paramDef && paramDef.type === 'boolean') {
        parsedParams[key] = value === 'true';
      } else {
        parsedParams[key] = value;
      }
    }

    // Execute command via Commands.execute
    // Pass sessionId explicitly to override auto-injected value (AI's own sessionId for sandboxing)
    // Pass context explicitly if provided (PersonaUser's enriched context with callerType)
    // This enables caller-adaptive command output (e.g., PersonaUsers receive media field in screenshot results)
    const commandParams: Record<string, any> = {
      sessionId,  // Pass AI's sessionId for proper attribution
      contextId,  // Some commands may use this (will be ignored if not needed)
      ...parsedParams
    };

    // Include enriched context if provided (enables caller-adaptive output)
    if (context) {
      commandParams.context = context;
    }

    const result = await Commands.execute(toolName, commandParams);

    // Check if command executed successfully (if result has success field)
    // Not all commands have success field, so we check first with type guard
    if (hasSuccessField(result) && !result.success) {
      return {
        toolName,
        success: false,
        error: this.stringifyError(result.error) || 'Command execution failed'
      };
    }

    // Extract media if present (screenshot, file/read, etc.)
    // Not all commands return media, so we check first with type guard
    const media: MediaItem[] | undefined = hasMediaField(result) && result.media
      ? (Array.isArray(result.media) ? result.media : [result.media])
      : undefined;

    // Format result based on command type
    const content = this.formatToolResult(toolName, result);

    return {
      toolName,
      success: true,
      content,
      media  // ← Preserve structured media
    };
  }

  /**
   * Convert any error value to a human-readable string
   * Prevents [object Object] in error messages
   */
  private stringifyError(error: unknown): string {
    if (error === undefined || error === null) {
      return 'Unknown error';
    }
    if (typeof error === 'string') {
      return error;
    }
    if (error instanceof Error) {
      return error.message;
    }
    if (typeof error === 'object' && error !== null) {
      const obj = error as Record<string, unknown>;
      if (typeof obj.message === 'string') return obj.message;
      if (typeof obj.error === 'string') return obj.error;
      if (typeof obj.errorMessage === 'string') return obj.errorMessage;
      try {
        const str = JSON.stringify(error);
        return str.length > 500 ? `${str.slice(0, 500)}...` : str;
      } catch {
        return 'Error object could not be serialized';
      }
    }
    return String(error);
  }

  /**
   * Format tool execution result for AI consumption
   */
  private formatToolResult(toolName: string, result: any): string {
    // Handle specific command patterns
    if (toolName === 'list' && result.commands) {
      return result.commands
        .map((cmd: any) => `${cmd.name} - ${cmd.description}`)
        .join('\n');
    }

    if (toolName.startsWith(DATA_COMMANDS.LIST) && result.items) {
      return `Collection: ${result.collection || 'unknown'}\nCount: ${result.count || result.items.length}\n\nResults:\n${JSON.stringify(result.items, null, 2)}`;
    }

    if (toolName.startsWith(DATA_COMMANDS.READ) && result.data) {
      return `Collection: ${result.collection || 'unknown'}\nID: ${result.id || 'unknown'}\n\nData:\n${JSON.stringify(result.data, null, 2)}`;
    }

    if (toolName === 'code/read' && result.content) {
      const lineRange = result.startLine && result.endLine
        ? ` (lines ${result.startLine}-${result.endLine})`
        : '';
      return `Path: ${result.path || 'unknown'}${lineRange}\n\n${result.content}`;
    }

    if (toolName === 'collaboration/chat/export' && result.markdown) {
      return `Exported ${result.messageCount || 0} messages:\n\n${result.markdown}`;
    }

    // Generic fallback - JSON stringify the result
    // But strip out 'context' field which is internal plumbing, not useful to AIs
    const { context: _, sessionId: __, ...cleanResult } = result;
    return JSON.stringify(cleanResult, null, 2);
  }

  /**
   * Generate tool descriptions for AI system prompt
   *
   * This replaces the hardcoded tool list in ChatRAGBuilder
   */
  generateToolDocumentation(): string {
    if (!this.initialized) {
      throw new Error('ToolRegistry not initialized - call initialize() first');
    }

    const tools = this.getAllTools();

    if (tools.length === 0) {
      return 'No tools available.';
    }

    // Group tools by category (first part of name)
    const categories = new Map<string, string[]>();
    for (const tool of tools) {
      const category = tool.name.split('/')[0];
      if (!categories.has(category)) {
        categories.set(category, []);
      }
      categories.get(category)!.push(tool.name);
    }

    // Compact list: just names grouped by category
    const compactList = Array.from(categories.entries())
      .map(([cat, names]) => `${cat}: ${names.join(', ')}`)
      .join('\n');

    // Keep few-shot examples - CRITICAL for small models
    const examples = `TOOL EXAMPLES:
<tool name="help"><command>data/list</command></tool>
<tool name="data/list"><collection>chat_messages</collection><limit>10</limit></tool>
<tool name="interface/screenshot"></tool>

DO NOT say "I'll use..." - just output the <tool> XML directly.`;

    return `TOOLS (${tools.length} available):
Format: <tool name="command"><param>value</param></tool>
Use: <tool name="help"><command>NAME</command></tool> for details

${compactList}

${examples}`;
  }
}
