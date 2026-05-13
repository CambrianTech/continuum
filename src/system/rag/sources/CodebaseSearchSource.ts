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
import { PromptTier } from '../shared/RAGSource';
import { getCodebaseIndexer } from '../services/CodebaseIndexer';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('CodebaseSearchSource', 'rag');

/** Maximum code results to inject into context */
const MAX_RESULTS = 5;

/** Minimum message length to trigger code search (skip "hi", "ok", etc.) */
const MIN_QUERY_LENGTH = 15;

/** Similarity threshold — only inject results that are genuinely relevant */
const RELEVANCE_THRESHOLD = 0.35;

/** Source-local latency budget. Code context is useful, but chat must not wait
 * on a cold or oversized index. The source degrades to empty context instead
 * of letting the whole persona response pipeline stall behind RAGComposer's
 * broader watchdog. */
const QUERY_TIMEOUT_MS = Number(process.env.CONTINUUM_CODEBASE_RAG_TIMEOUT_MS ?? 4_000);

const TECHNICAL_QUERY_PATTERN = new RegExp([
  '\\b(code|codebase|repo|repository|file|files|function|class|interface|type|module|import|export)\\b',
  '\\b(bug|error|exception|stack|trace|crash|failing|failure|fix|debug|compile|build)\\b',
  '\\b(unit|integration|e2e|regression)\\s+tests?\\b',
  '\\btests?\\s+(failed|failing|fail|red|broken|pass|passing|green)\\b',
  '\\b(cargo|npm|pnpm|yarn|pytest|vitest|jest|playwright)\\s+test\\b',
  '\\b(refactor|architecture|architect|implement|implementation|api|endpoint|schema|database|docker)\\b',
  '\\b(rust|typescript|javascript|tsx|jsx|node|python|cargo|npm|sql|sqlite|postgres)\\b',
  '`[^`]+`',
  '[\\w./-]+\\.(ts|tsx|js|jsx|rs|py|toml|json|md|sql|sh|ps1)\\b',
].join('|'), 'i');

export class CodebaseSearchSource implements RAGSource {
  readonly name = 'codebase-search';
  readonly tier = PromptTier.VOLATILE;
  readonly priority = 55;
  readonly defaultBudgetPercent = 8;
  readonly isShared = true;

  isApplicable(context: RAGSourceContext): boolean {
    const currentMessage = context.options?.currentMessage?.content;
    if (!currentMessage || typeof currentMessage !== 'string') return false;

    // Recipe-owned RAG activation is authoritative. If a queue item or room
    // recipe explicitly asks for codebase-search, provide it even when the
    // surface text is terse ("fix this", "same bug").
    if (context.activeSources?.includes(this.name)) return true;

    if (currentMessage.trim().length < MIN_QUERY_LENGTH) return false;

    // Default chat should stay conversational. Pulling semantic code search
    // for every ordinary room message turns one human prompt into N expensive
    // index queries across personas and was observed to wedge chat behind a
    // 30s RAG timeout. Codebase context is activated by technical intent.
    return TECHNICAL_QUERY_PATTERN.test(currentMessage);
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<Omit<RAGSection, 'tier'>> {
    const startTime = Date.now();
    const query = context.options?.currentMessage?.content as string;

    try {
      const indexer = getCodebaseIndexer();
      const results = await this.withQueryTimeout(indexer.query(query, MAX_RESULTS), query);

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

  private async withQueryTimeout<T>(queryPromise: Promise<T>, query: string): Promise<T> {
    let timer: ReturnType<typeof setTimeout> | null = null;
    try {
      const timeout = new Promise<never>((_, reject) => {
        timer = setTimeout(() => {
          reject(new Error(`codebase search exceeded ${QUERY_TIMEOUT_MS}ms for "${query.slice(0, 40)}..."`));
        }, QUERY_TIMEOUT_MS);
        timer.unref?.();
      });
      return await Promise.race([queryPromise, timeout]);
    } finally {
      if (timer) clearTimeout(timer);
    }
  }
}
