/**
 * CodebaseRAGBuilder - Builds LLM context from source code and documentation
 *
 * Loads:
 * - TypeScript files (interfaces, classes, functions)
 * - Markdown documentation
 * - Code structure and relationships
 * - Embeddings for semantic search
 *
 * Used by PersonaUser to answer questions about the codebase architecture
 */

import { RAGBuilder } from '../shared/RAGBuilder';
import type {
  RAGContext,
  RAGBuildOptions,
  RAGDomain,
  LLMMessage,
  RAGArtifact,
  PersonaIdentity,
  PersonaMemory
} from '../shared/RAGTypes';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { DataDaemon } from '../../../daemons/data-daemon/shared/DataDaemon';
import { UserEntity } from '../../data/entities/UserEntity';
import type { CodeIndexEntry } from '../shared/CodebaseTypes';
import { COLLECTIONS } from '../../shared/Constants';

/**
 * Codebase-specific RAG builder
 * Converts source code and documentation into LLM-ready context
 */
export class CodebaseRAGBuilder extends RAGBuilder {
  readonly domain: RAGDomain = 'code';

  /**
   * Build RAG context from a codebase query
   *
   * @param contextId - Query text or scope path (e.g., "/system/user/")
   * @param personaId - The persona requesting context
   * @param options - Configuration (result limit, etc.)
   */
  async buildContext(
    contextId: UUID,  // Query text or scope path
    personaId: UUID,
    options?: RAGBuildOptions
  ): Promise<RAGContext> {
    const startTime = Date.now();

    const maxResults = options?.maxMessages ?? 10;  // Reuse maxMessages for result limit

    // 1. Load persona identity
    const identity = await this.loadPersonaIdentity(personaId);

    // 2. Parse contextId - could be query text or just used as context identifier
    // For now, treat contextId as the query text itself
    const query = contextId;

    // 3. Query indexed codebase
    const codeResults = await this.queryCodebase(query, maxResults);

    // 4. Build conversation history from code results
    const conversationHistory = this.buildConversationFromResults(codeResults, query);

    // 5. Extract artifacts (code snippets as artifacts)
    const artifacts = this.buildArtifactsFromResults(codeResults);

    // 6. No private memories for code queries (yet)
    const privateMemories: PersonaMemory[] = [];

    const ragContext: RAGContext = {
      domain: 'code',
      contextId,
      personaId,
      identity,
      conversationHistory,
      artifacts,
      privateMemories,
      metadata: {
        messageCount: conversationHistory.length,
        artifactCount: artifacts.length,
        memoryCount: 0,
        builtAt: new Date(),
        recipeId: 'codebase-query',
        recipeName: 'Codebase Query'
      }
    };

    const durationMs = Date.now() - startTime;
    console.log(`📚 CodebaseRAGBuilder: Built context in ${durationMs}ms (${codeResults.length} results)`);

    return ragContext;
  }

  getDescription(): string {
    return 'Codebase indexer with TypeScript and Markdown support';
  }

  /**
   * Load persona identity from UserEntity
   */
  private async loadPersonaIdentity(personaId: UUID): Promise<PersonaIdentity> {
    try {
      const user = await DataDaemon.read<UserEntity>(UserEntity.collection, personaId);

      if (!user) {
        console.warn(`⚠️ CodebaseRAGBuilder: Could not load persona ${personaId}, using defaults`);
        return {
          name: 'Code Expert',
          systemPrompt: 'You are an expert at analyzing and explaining code architecture. Provide clear, accurate answers with file references.'
        };
      }

      return {
        name: user.displayName,
        bio: user.profile?.bio,
        role: user.type,
        systemPrompt: this.buildSystemPrompt(user),
        capabilities: user.capabilities ? Object.keys(user.capabilities) : []
      };
    } catch (error) {
      console.error(`❌ CodebaseRAGBuilder: Error loading persona identity:`, error);
      return {
        name: 'Code Expert',
        systemPrompt: 'You are an expert at analyzing and explaining code architecture. Provide clear, accurate answers with file references.'
      };
    }
  }

  /**
   * Build system prompt for codebase queries
   */
  private buildSystemPrompt(user: UserEntity): string {
    const name = user.displayName;
    const bio = user.profile?.bio ?? user.shortDescription ?? '';

    return `IDENTITY: You are ${name}${bio ? `, ${bio}` : ''}.

ROLE: You are a code architecture expert who helps developers understand this codebase.

RESPONSE FORMAT:
1. Provide clear, accurate answers based on the code context provided
2. ALWAYS include file references in format: filename.ts:line_number
3. Explain WHY code is structured a certain way, not just WHAT it does
4. Use 1-3 sentences for simple questions, longer for complex architecture explanations
5. If you don't have enough context to answer accurately, say so

EXAMPLE GOOD RESPONSES:
Q: "Why does PersonaUser have inbox?"
A: "PersonaUser.inbox is a priority queue (PersonaInbox.ts:45-120) that manages incoming tasks. It enables autonomous servicing - PersonaUser polls inbox on adaptive cadence (3s→5s→7s) based on energy/mood state. This decouples task arrival from processing, allowing state-aware engagement."

Q: "How do Commands work?"
A: "Commands.execute() (Commands.ts:89-156) uses TypeScript inference to provide type-safe command execution. The CommandRegistry (CommandRegistry.ts:23-78) maps command names to handler implementations. This creates a universal primitive that works across browser/server/CLI environments."`;
  }

  /**
   * Query indexed codebase
   * TODO: Implement actual vector search with embeddings
   * For now, returns mock results
   */
  private async queryCodebase(query: string, maxResults: number): Promise<CodeIndexEntry[]> {
    try {
      // TODO: Query code_index collection with vector similarity search
      const result = await DataDaemon.query<CodeIndexEntry>({
        collection: COLLECTIONS.CODE_INDEX,
        filter: {}, // TODO: Add vector similarity filter
        sort: [{ field: 'timestamp', direction: 'desc' }],
        limit: maxResults
      });

      if (!result.success || !result.data || result.data.length === 0) {
        console.log(`ℹ️ CodebaseRAGBuilder: No results for query "${query.slice(0, 50)}..."`);
        return [];
      }

      const entries = result.data.map(record => record.data);
      console.log(`✅ CodebaseRAGBuilder: Found ${entries.length} results for query`);
      return entries;
    } catch (error) {
      console.error(`❌ CodebaseRAGBuilder: Error querying codebase:`, error);
      return [];
    }
  }

  /**
   * Build conversation history from code results
   * Format: User asks question, System provides code context
   */
  private buildConversationFromResults(results: CodeIndexEntry[], query: string): LLMMessage[] {
    const messages: LLMMessage[] = [];

    // Add system message with code context
    if (results.length > 0) {
      const contextText = results
        .map(entry => {
          const lines = entry.startLine && entry.endLine
            ? `:${entry.startLine}-${entry.endLine}`
            : '';
          return `\n## ${entry.filePath}${lines}\n${entry.content}`;
        })
        .join('\n\n---\n');

      messages.push({
        role: 'system',
        content: `Relevant code context for question:\n${contextText}`
      });
    } else {
      messages.push({
        role: 'system',
        content: `No indexed code found for this query. The codebase may not be indexed yet. Suggest running: ./jtag rag/index-codebase`
      });
    }

    // Add user's question
    messages.push({
      role: 'user',
      content: query,
      timestamp: Date.now()
    });

    return messages;
  }

  /**
   * Build artifacts from code results
   * Each code snippet becomes an artifact with metadata
   */
  private buildArtifactsFromResults(results: CodeIndexEntry[]): RAGArtifact[] {
    return results.map(entry => ({
      type: 'file' as const,
      content: entry.content,
      metadata: {
        filePath: entry.filePath,
        startLine: entry.startLine,
        endLine: entry.endLine,
        exportType: entry.exportType,
        exportName: entry.exportName,
        relevanceScore: entry.relevanceScore ?? 0
      }
    }));
  }
}
