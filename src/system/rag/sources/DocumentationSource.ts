/**
 * DocumentationSource - Injects system documentation awareness into persona RAG context
 *
 * Gives personas a chapter map of all system docs so they know:
 * - What documentation exists (organized by topic)
 * - How to drill down (utilities/docs/* workflow)
 * - Which chapters are relevant to the current conversation
 *
 * Without this, personas have NO idea what Continuum is or how anything works.
 * They can't help users with architecture questions, can't find docs, can't learn.
 *
 * Priority 35 - Low-medium. Always useful context but not critical for every message.
 * Budget 5% (~250 tokens for the chapter map).
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import { Logger } from '../../core/logging/Logger';
import * as fs from 'fs/promises';
import * as path from 'path';

const log = Logger.create('DocumentationSource', 'rag');

/** Chapter descriptor for the documentation map */
interface DocChapter {
  readonly directory: string;
  readonly description: string;
  count: number;
}

/** Static chapter definitions — descriptions of what each docs/ subdirectory covers */
const DOC_CHAPTERS: readonly Omit<DocChapter, 'count'>[] = [
  { directory: 'genome', description: 'LoRA training, inference, adapters, fine-tuning pipeline' },
  { directory: 'personas', description: 'Cognition, memory, identity, academy, autonomous loop' },
  { directory: 'sentinel', description: 'Pipeline engine, coding AI, step types' },
  { directory: 'activities', description: 'Rooms, walls, recipes, collaboration' },
  { directory: 'positron', description: 'UI framework, widgets, state management' },
  { directory: 'live', description: 'Voice, video, VAD, media, avatars' },
  { directory: 'governance', description: 'Democratic AI society, ethics, voting' },
  { directory: 'infrastructure', description: 'Daemons, data, commands, events, Rust IPC, ORM' },
  { directory: 'grid', description: 'P2P mesh, marketplace' },
  { directory: 'planning', description: 'Roadmaps, audits, phases' },
  { directory: 'testing', description: 'Test strategies, integration tests' },
  { directory: 'papers', description: 'Research papers and references' },
] as const;

export class DocumentationSource implements RAGSource {
  readonly name = 'documentation';
  readonly priority = 35;
  readonly defaultBudgetPercent = 5;

  // Shared cache + single-flight coalescing — all personas share one filesystem scan.
  // Without single-flight, 5+ personas hit buildChapterMap() simultaneously on cold start,
  // each doing redundant recursive readdir. Single-flight ensures exactly ONE scan.
  private static _cachedChapterMap: string | null = null;
  private static _cacheGeneratedAt = 0;
  private static readonly CACHE_TTL_MS = 30 * 60 * 1000; // 30 minutes
  private static _inflight: Promise<string> | null = null;

  isApplicable(_context: RAGSourceContext): boolean {
    // Always applicable — every persona benefits from knowing docs exist
    return true;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    try {
      const chapterMap = await this.getOrBuildChapterMap();
      const tokenCount = this.estimateTokens(chapterMap);
      const budgetTokens = Math.floor(allocatedBudget);

      // If over budget, return minimal version
      const finalPrompt = tokenCount > budgetTokens
        ? this.buildMinimalPrompt()
        : chapterMap;

      const finalTokens = this.estimateTokens(finalPrompt);

      log.debug(`Loaded documentation map (${finalTokens} tokens) for persona ${context.personaId.slice(0, 8)}`);

      return {
        sourceName: this.name,
        tokenCount: finalTokens,
        loadTimeMs: performance.now() - startTime,
        systemPromptSection: finalPrompt,
        metadata: {
          budgetRespected: finalTokens <= budgetTokens,
        },
      };
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      log.error(`Failed to load documentation context: ${message}`);
      return {
        sourceName: this.name,
        tokenCount: 0,
        loadTimeMs: performance.now() - startTime,
        metadata: { error: message },
      };
    }
  }

  private async getOrBuildChapterMap(): Promise<string> {
    const now = Date.now();
    if (
      DocumentationSource._cachedChapterMap &&
      (now - DocumentationSource._cacheGeneratedAt) < DocumentationSource.CACHE_TTL_MS
    ) {
      return DocumentationSource._cachedChapterMap;
    }

    // Single-flight: if another caller is already building, piggyback on that promise
    if (DocumentationSource._inflight) {
      return DocumentationSource._inflight;
    }

    const promise = this.buildChapterMap().then(result => {
      DocumentationSource._cachedChapterMap = result;
      DocumentationSource._cacheGeneratedAt = Date.now();
      return result;
    }).finally(() => {
      DocumentationSource._inflight = null;
    });

    DocumentationSource._inflight = promise;
    return promise;
  }

  private async buildChapterMap(): Promise<string> {
    const docsRoot = path.join(process.cwd(), 'docs');

    // Count files per chapter in parallel — all 12 directories scanned concurrently
    const chapterPromises = DOC_CHAPTERS.map(async (chapter) => {
      const chapterDir = path.join(docsRoot, chapter.directory);
      const count = await this.countMarkdownFiles(chapterDir);
      return count > 0 ? { ...chapter, count } as DocChapter : null;
    });

    const chapters = (await Promise.all(chapterPromises)).filter((c): c is DocChapter => c !== null);

    const lines: string[] = [
      '## System Documentation',
      'Architecture docs organized by chapter. Use utilities/docs/* tools to explore.',
      '',
    ];

    for (const chapter of chapters) {
      lines.push(`- **${chapter.directory}/** — ${chapter.description} (${chapter.count} docs)`);
    }

    lines.push('');
    lines.push('### How to Explore Documentation');
    lines.push('1. `utilities/docs/search --pattern="keyword"` — Find docs mentioning a topic');
    lines.push('2. `utilities/docs/list` — Browse all docs with section headings');
    lines.push('3. `utilities/docs/read --doc="chapter/doc-name" --toc` — See table of contents');
    lines.push('4. `utilities/docs/read --doc="chapter/doc-name" --section="Section Title"` — Read a specific section');
    lines.push('');
    lines.push('Search first, then drill down. Do not read entire documents — use --section for targeted reading.');

    return lines.join('\n');
  }

  private async countMarkdownFiles(dir: string): Promise<number> {
    try {
      let count = 0;
      const entries = await fs.readdir(dir, { withFileTypes: true });
      for (const entry of entries) {
        if (entry.isFile() && entry.name.endsWith('.md')) {
          count++;
        } else if (entry.isDirectory() && !entry.name.startsWith('.')) {
          count += await this.countMarkdownFiles(path.join(dir, entry.name));
        }
      }
      return count;
    } catch {
      return 0;
    }
  }

  private buildMinimalPrompt(): string {
    return 'System docs available in docs/. Use utilities/docs/search --pattern="keyword" to find topics, utilities/docs/read --doc="name" --section="Title" to read.';
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
