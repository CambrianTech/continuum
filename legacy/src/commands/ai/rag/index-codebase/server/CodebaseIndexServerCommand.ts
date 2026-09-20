/**
 * Codebase Index Server Command
 *
 * Walks the repo, chunks files by declarations, generates embeddings
 * via Rust IPC, and stores in code_index collection for semantic search.
 */

import { CodebaseIndexCommand } from '../shared/CodebaseIndexCommand';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { CodebaseIndexParams, CodebaseIndexResult } from '../shared/CodebaseIndexTypes';
import { getCodebaseIndexer } from '../../../../../system/rag/services/CodebaseIndexer';
import * as path from 'path';

export class CodebaseIndexServerCommand extends CodebaseIndexCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/index-codebase', context, subpath, commander);
  }

  async execute(params: CodebaseIndexParams): Promise<CodebaseIndexResult> {
    const startTime = Date.now();

    try {
      const indexer = getCodebaseIndexer();

      // Resolve paths relative to src/ (cwd)
      const srcRoot = process.cwd();
      const paths = params.paths?.length > 0
        ? params.paths.map(p => path.resolve(srcRoot, p))
        : [srcRoot];

      console.log(`Indexing ${paths.length} path(s) from ${srcRoot}`);

      // Index each path
      let totalFiles = 0;
      let totalEntries = 0;
      const allErrors: Array<{ file: string; error: string }> = [];
      const allFiles: CodebaseIndexResult['files'] = [];

      for (const indexPath of paths) {
        const result = await indexer.index(indexPath, {
          recursive: params.recursive ?? true,
          clearExisting: paths.indexOf(indexPath) === 0, // Only clear on first path
        });

        totalFiles += result.filesIndexed;
        totalEntries += result.entriesCreated;
        allErrors.push(...result.errors);
      }

      const durationMs = Date.now() - startTime;
      console.log(`Codebase indexing complete: ${totalEntries} entries from ${totalFiles} files in ${(durationMs / 1000).toFixed(1)}s`);

      return {
        success: allErrors.length === 0,
        summary: {
          filesIndexed: totalFiles,
          filesSkipped: 0,
          entriesCreated: totalEntries,
          entriesUpdated: 0,
          embeddingsGenerated: totalEntries,
          durationMs,
        },
        files: allFiles,
        errors: allErrors.length > 0 ? allErrors : undefined,
        context: this.context,
        sessionId: params.sessionId,
      };
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        summary: {
          filesIndexed: 0,
          filesSkipped: 0,
          entriesCreated: 0,
          entriesUpdated: 0,
          embeddingsGenerated: 0,
          durationMs: Date.now() - startTime,
        },
        files: [],
        context: this.context,
        sessionId: params.sessionId,
      };
    }
  }
}
