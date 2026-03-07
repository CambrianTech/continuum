/**
 * CodebaseIndexer — Walks the repo, chunks TypeScript/Markdown files,
 * generates embeddings via Rust IPC, and stores in the code_index collection.
 *
 * Architecture:
 * - File discovery: glob patterns for .ts, .md, .js files
 * - Chunking: splits files into meaningful units (classes, functions, sections)
 * - Embedding: fastembed via Rust IPC (AllMiniLML6V2, 384 dims, ~5ms per embed)
 * - Storage: ORM → code_index collection with embedding vectors
 * - Query: vector similarity search via embeddingTopK
 *
 * All heavy work (embedding generation) runs in Rust off main thread.
 */

import * as fs from 'fs';
import * as path from 'path';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import { CodeIndexEntity } from '../../data/entities/CodeIndexEntity';
import type { IndexingResult, CodeExportType } from '../shared/CodebaseTypes';
import type { CodeIndexEntry } from '../shared/CodebaseTypes';
import type { RustCoreIPCClient as RustCoreIPCClientType } from '../../../workers/continuum-core/bindings/RustCoreIPC';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('CodebaseIndexer', 'rag');

/** Maximum content length per chunk (chars). Longer chunks are split. */
const MAX_CHUNK_CHARS = 2000;

/** Batch size for embedding generation — one Rust IPC call per batch */
const EMBEDDING_BATCH_SIZE = 64;

/** File extensions to index */
const INDEXABLE_EXTENSIONS = new Set(['.ts', '.md', '.js']);

/** Directories to skip */
const SKIP_DIRS = new Set([
  'node_modules', '.git', 'dist', 'build', '.continuum',
  'coverage', '.next', '.cache', 'generated-command-schemas.json',
]);

// ============================================================================
// Chunk types
// ============================================================================

interface FileChunk {
  filePath: string;
  fileType: 'typescript' | 'markdown' | 'javascript';
  content: string;
  startLine: number;
  endLine: number;
  exportType?: CodeExportType;
  exportName?: string;
}

// ============================================================================
// CodebaseIndexer
// ============================================================================

export class CodebaseIndexer {
  private ipcClient: RustCoreIPCClientType | null = null;

  /**
   * Index a directory tree, generating embeddings and storing in code_index.
   *
   * @param rootDir - Root directory to index (e.g., project src/)
   * @param options - Indexing options
   * @returns Indexing result with counts
   */
  async index(rootDir: string, options?: {
    fileTypes?: string[];
    recursive?: boolean;
    clearExisting?: boolean;
  }): Promise<IndexingResult> {
    const startTime = Date.now();
    const errors: Array<{ file: string; error: string }> = [];

    log.info(`Starting codebase indexing: ${rootDir}`);

    // 1. Clear existing index if requested
    if (options?.clearExisting) {
      await this.clearIndex();
    }

    // 2. Discover files
    const files = this.discoverFiles(rootDir, options?.recursive ?? true);
    log.info(`Discovered ${files.length} indexable files`);

    // 3. Chunk all files
    const allChunks: FileChunk[] = [];
    for (const filePath of files) {
      try {
        const chunks = await this.chunkFile(filePath, rootDir);
        allChunks.push(...chunks);
      } catch (err) {
        errors.push({ file: filePath, error: String(err) });
      }
    }
    log.info(`Generated ${allChunks.length} chunks from ${files.length} files`);

    // 4. Generate embeddings in batches via Rust IPC
    const ipc = await this.getIPC();
    let entriesCreated = 0;

    for (let i = 0; i < allChunks.length; i += EMBEDDING_BATCH_SIZE) {
      const batch = allChunks.slice(i, i + EMBEDDING_BATCH_SIZE);
      const texts = batch.map(c => this.chunkToEmbeddingText(c));

      try {
        // Single Rust IPC call for entire batch — fastembed handles batches natively
        const embeddings = await ipc.embeddingGenerateBatch(texts);

        for (let j = 0; j < batch.length; j++) {
          const chunk = batch[j];
          const embedding = embeddings[j];

          const entry = new CodeIndexEntity();
          entry.filePath = chunk.filePath;
          entry.fileType = chunk.fileType;
          entry.content = chunk.content;
          entry.startLine = chunk.startLine;
          entry.endLine = chunk.endLine;
          entry.exportType = chunk.exportType;
          entry.exportName = chunk.exportName;
          entry.embedding = embedding.embedding;
          entry.embeddingModel = embedding.model;
          entry.lastIndexed = new Date();

          await ORM.store(CodeIndexEntity.collection, entry, false, 'default');
          entriesCreated++;
        }

        log.info(`Indexed batch ${i}-${i + batch.length}/${allChunks.length}`);
      } catch (err) {
        log.error(`Embedding batch ${i}-${i + batch.length} failed: ${err}`);
        errors.push({ file: `batch-${i}`, error: String(err) });
      }
    }

    const durationMs = Date.now() - startTime;
    log.info(`Indexing complete: ${entriesCreated} entries in ${(durationMs / 1000).toFixed(1)}s (${errors.length} errors)`);

    return {
      success: errors.length === 0,
      filesIndexed: files.length,
      entriesCreated,
      entriesUpdated: 0,
      errors,
      durationMs,
    };
  }

  /**
   * Query the code index using semantic vector search.
   *
   * @param query - Natural language query
   * @param maxResults - Maximum results to return
   * @returns Matching code entries sorted by relevance
   */
  async query(queryText: string, maxResults: number = 10): Promise<CodeIndexEntry[]> {
    const ipc = await this.getIPC();

    // 1. Generate query embedding
    const queryEmbedding = await ipc.embeddingGenerate(queryText);

    // 2. Load all indexed entries with embeddings
    const result = await ORM.query<CodeIndexEntity>({
      collection: CodeIndexEntity.collection,
      filter: {},
      limit: 1000,  // Load all for vector search
    }, 'default');

    if (!result.success || !result.data || result.data.length === 0) {
      return [];
    }

    const entries = result.data
      .map(r => r.data)
      .filter(e => e.embedding && e.embedding.length > 0);
    if (entries.length === 0) return [];

    // 3. Use Rust top-K for fast cosine similarity
    const targets = entries.map(e => e.embedding!);
    const topK = await ipc.embeddingTopK(
      queryEmbedding.embedding,
      targets,
      maxResults,
      0.3,  // minimum similarity threshold
    );

    // 4. Map results back to entries with relevance scores
    return topK.results.map(r => {
      const entry = entries[r.index];
      return {
        ...entry,
        relevanceScore: r.similarity,
      } as CodeIndexEntry;
    });
  }

  /**
   * Clear the entire code index.
   */
  async clearIndex(): Promise<void> {
    try {
      const result = await ORM.query<CodeIndexEntity>({
        collection: CodeIndexEntity.collection,
        filter: {},
        limit: 10000,
      }, 'default');

      if (result.success && result.data) {
        for (const record of result.data) {
          await ORM.remove(CodeIndexEntity.collection, record.data.id as UUID, false, 'default');
        }
        log.info(`Cleared ${result.data.length} entries from code index`);
      }
    } catch (err) {
      log.warn(`Failed to clear code index: ${err}`);
    }
  }

  // ── File discovery ────────────────────────────────────────────────────

  private discoverFiles(dir: string, recursive: boolean): string[] {
    const files: string[] = [];
    this.walkDir(dir, recursive, files);
    return files;
  }

  private walkDir(dir: string, recursive: boolean, accumulator: string[]): void {
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(dir, { withFileTypes: true });
    } catch {
      return;
    }

    for (const entry of entries) {
      if (SKIP_DIRS.has(entry.name)) continue;

      const fullPath = path.join(dir, entry.name);

      if (entry.isDirectory() && recursive) {
        this.walkDir(fullPath, recursive, accumulator);
      } else if (entry.isFile()) {
        const ext = path.extname(entry.name);
        if (INDEXABLE_EXTENSIONS.has(ext)) {
          accumulator.push(fullPath);
        }
      }
    }
  }

  // ── File chunking ─────────────────────────────────────────────────────

  private async chunkFile(filePath: string, rootDir: string): Promise<FileChunk[]> {
    const content = await fs.promises.readFile(filePath, 'utf-8');
    const ext = path.extname(filePath);
    const relativePath = path.relative(rootDir, filePath);

    if (ext === '.md') {
      return this.chunkMarkdown(content, relativePath);
    }
    return this.chunkTypeScript(content, relativePath, ext === '.js' ? 'javascript' : 'typescript');
  }

  /**
   * Chunk TypeScript/JavaScript by top-level declarations.
   * Each class, interface, function, or const export becomes a chunk.
   */
  private chunkTypeScript(
    content: string,
    filePath: string,
    fileType: 'typescript' | 'javascript',
  ): FileChunk[] {
    const lines = content.split('\n');
    const chunks: FileChunk[] = [];

    // Regex patterns for top-level declarations
    const declarationPattern = /^(?:export\s+)?(?:default\s+)?(?:abstract\s+)?(class|interface|type|function|const|enum|namespace)\s+(\w+)/;

    let currentChunk: { startLine: number; exportType: CodeExportType; exportName: string } | null = null;
    let braceDepth = 0;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const match = line.match(declarationPattern);

      if (match && braceDepth === 0) {
        // Save previous chunk if exists
        if (currentChunk) {
          this.saveChunk(chunks, lines, filePath, fileType, currentChunk.startLine, i - 1, currentChunk.exportType, currentChunk.exportName);
        }

        currentChunk = {
          startLine: i + 1,
          exportType: match[1] as CodeExportType,
          exportName: match[2],
        };
      }

      // Track brace depth for multi-line declarations
      for (const ch of line) {
        if (ch === '{') braceDepth++;
        if (ch === '}') braceDepth = Math.max(0, braceDepth - 1);
      }
    }

    // Save last chunk
    if (currentChunk) {
      this.saveChunk(chunks, lines, filePath, fileType, currentChunk.startLine, lines.length, currentChunk.exportType, currentChunk.exportName);
    }

    // If no declarations found, chunk the whole file
    if (chunks.length === 0 && content.trim().length > 0) {
      chunks.push({
        filePath,
        fileType,
        content: content.slice(0, MAX_CHUNK_CHARS),
        startLine: 1,
        endLine: Math.min(lines.length, 60),
      });
    }

    return chunks;
  }

  /**
   * Chunk Markdown by headings (## and ###).
   */
  private chunkMarkdown(content: string, filePath: string): FileChunk[] {
    const lines = content.split('\n');
    const chunks: FileChunk[] = [];
    let sectionStart = 0;
    let sectionName = filePath;

    for (let i = 0; i < lines.length; i++) {
      const headingMatch = lines[i].match(/^(#{1,3})\s+(.+)/);
      if (headingMatch) {
        if (i > sectionStart) {
          // Save previous section
          const sectionContent = lines.slice(sectionStart, i).join('\n').trim();
          if (sectionContent.length > 20) {
            chunks.push({
              filePath,
              fileType: 'markdown',
              content: sectionContent.slice(0, MAX_CHUNK_CHARS),
              startLine: sectionStart + 1,
              endLine: i,
              exportType: 'markdown-section',
              exportName: sectionName,
            });
          }
          sectionStart = i;
        }
        sectionName = headingMatch[2].trim();
      }
    }

    // Save last section
    const lastContent = lines.slice(sectionStart).join('\n').trim();
    if (lastContent.length > 20) {
      chunks.push({
        filePath,
        fileType: 'markdown',
        content: lastContent.slice(0, MAX_CHUNK_CHARS),
        startLine: sectionStart + 1,
        endLine: lines.length,
        exportType: 'markdown-section',
        exportName: sectionName,
      });
    }

    return chunks;
  }

  private saveChunk(
    chunks: FileChunk[],
    lines: string[],
    filePath: string,
    fileType: 'typescript' | 'javascript',
    startLine: number,
    endLine: number,
    exportType: CodeExportType,
    exportName: string,
  ): void {
    const content = lines.slice(startLine - 1, endLine).join('\n').trim();
    if (content.length < 10) return;

    chunks.push({
      filePath,
      fileType,
      content: content.slice(0, MAX_CHUNK_CHARS),
      startLine,
      endLine,
      exportType,
      exportName,
    });
  }

  // ── Embedding text ────────────────────────────────────────────────────

  /**
   * Convert a chunk into text optimized for embedding generation.
   * Includes file path and export name for context.
   */
  private chunkToEmbeddingText(chunk: FileChunk): string {
    const parts: string[] = [];
    if (chunk.exportName) {
      parts.push(`${chunk.exportType ?? ''} ${chunk.exportName} in ${chunk.filePath}`);
    } else {
      parts.push(chunk.filePath);
    }
    parts.push(chunk.content.slice(0, 1500)); // Limit for embedding input
    return parts.join('\n');
  }

  // ── IPC connection ────────────────────────────────────────────────────

  private async getIPC(): Promise<RustCoreIPCClientType> {
    if (this.ipcClient) return this.ipcClient;

    const { RustCoreIPCClient, getContinuumCoreSocketPath } = await import('../../../workers/continuum-core/bindings/RustCoreIPC');
    this.ipcClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
    await this.ipcClient.connect();
    return this.ipcClient;
  }
}

// ── Singleton ─────────────────────────────────────────────────────────

let instance: CodebaseIndexer | null = null;

export function getCodebaseIndexer(): CodebaseIndexer {
  if (!instance) {
    instance = new CodebaseIndexer();
  }
  return instance;
}
