/**
 * CodebaseSearchSource - Injects semantically relevant code into persona RAG context
 *
 * When a persona receives a message about code, architecture, or technical topics,
 * this source queries the code_index (populated by CodebaseIndexer) using vector
 * similarity search and injects the most relevant code snippets into the LLM context.
 *
 * Uses the current user message as the search query against 384-dim fastembed
 * embeddings via Rust IPC (embeddingGenerate + embeddingTopK).
 *
 * Priority 55 - Medium-high. Valuable for technical conversations, skipped for
 * purely social chatter. Token cost scales with results (typically 500-1500 tokens).
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import { getCodebaseIndexer } from '../services/CodebaseIndexer';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('CodebaseSearchSource', 'rag');

/** Maximum code results to inject into context */
const MAX_RESULTS = 5;

/** Minimum message length to trigger code search (skip "hi", "ok", etc.) */
const MIN_QUERY_LENGTH = 15;

/** Similarity threshold — only inject results that are genuinely relevant */
const RELEVANCE_THRESHOLD = 0.35;

export class CodebaseSearchSource implements RAGSource {
  readonly name = 'codebase-search';
  readonly priority = 55;
  readonly defaultBudgetPercent = 8;

  isApplicable(context: RAGSourceContext): boolean {
    // Always applicable if there's a substantive message.
    // The persona's mind decides what context matters — we just provide the capability.
    // If results aren't relevant (low cosine similarity), the query returns empty
    // and costs nothing in the token budget.
    const currentMessage = context.options?.currentMessage?.content;
    if (!currentMessage || typeof currentMessage !== 'string') return false;
    return currentMessage.length >= MIN_QUERY_LENGTH;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = Date.now();
    const query = context.options?.currentMessage?.content as string;

    try {
      const indexer = getCodebaseIndexer();
      const results = await indexer.query(query, MAX_RESULTS);

      // Filter by relevance — only inject results the persona would actually find useful
      const relevant = results.filter(r => (r.relevanceScore ?? 0) >= RELEVANCE_THRESHOLD);

      if (relevant.length === 0) {
        return {
          sourceName: this.name,
          tokenCount: 0,
          loadTimeMs: Date.now() - startTime,
        };
      }

      // Build a system prompt section with relevant code
      const codeContext = relevant
        .map(entry => {
          const location = entry.startLine && entry.endLine
            ? `:${entry.startLine}-${entry.endLine}`
            : '';
          const label = entry.exportName
            ? `${entry.exportType ?? ''} ${entry.exportName}`
            : entry.filePath;
          const score = entry.relevanceScore?.toFixed(3) ?? '?';
          return `### ${label} (${entry.filePath}${location}) [${score}]\n\`\`\`${entry.fileType}\n${entry.content}\n\`\`\``;
        })
        .join('\n\n');

      const section = `\n## Relevant Codebase Context\nThe following code is semantically relevant to the current conversation:\n\n${codeContext}`;

      const tokenCount = Math.ceil(section.length / 4);

      log.info(`Found ${relevant.length}/${results.length} relevant code results for "${query.slice(0, 40)}..." (${tokenCount} tokens, ${Date.now() - startTime}ms)`);

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs: Date.now() - startTime,
        systemPromptSection: section,
      };
    } catch (err) {
      log.warn(`Code search failed: ${err}`);
      return {
        sourceName: this.name,
        tokenCount: 0,
        loadTimeMs: Date.now() - startTime,
      };
    }
  }
}
