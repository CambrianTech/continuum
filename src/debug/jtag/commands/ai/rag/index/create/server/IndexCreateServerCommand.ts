/**
 * Index Create Server Command
 *
 * Server-side implementation for storing code index entries
 */

import { IndexCreateCommand } from '../shared/IndexCreateCommand';
import type { JTAGContext } from '../../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../../daemons/command-daemon/shared/CommandBase';
import type { IndexCreateParams, IndexCreateResult } from '../shared/IndexCreateTypes';
import type { DataCreateResult } from '../../../../../data/create/shared/DataCreateTypes';
import { CodeIndexEntity } from '../../../../../../system/data/entities/CodeIndexEntity';
import { Commands } from '../../../../../../system/core/shared/Commands';

export class IndexCreateServerCommand extends IndexCreateCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/index/create', context, subpath, commander);
  }

  async execute(params: IndexCreateParams): Promise<IndexCreateResult> {
    const startTime = Date.now();

    try {
      console.log(`📇 Creating code index entry for ${params.filePath}`);

      // Create CodeIndexEntity from params
      const entry = new CodeIndexEntity();
      entry.filePath = params.filePath;
      entry.fileType = params.fileType;
      entry.content = params.content;
      entry.summary = params.summary;
      entry.startLine = params.startLine;
      entry.endLine = params.endLine;
      entry.exportType = params.exportType;
      entry.exportName = params.exportName;
      entry.embedding = params.embedding;
      entry.embeddingModel = params.embeddingModel;
      entry.imports = params.imports;
      entry.exports = params.exports;
      entry.tags = params.tags;
      entry.lastIndexed = new Date();

      // Validate the entity
      const validation = entry.validate();
      if (!validation.success) {
        const durationMs = Date.now() - startTime;
        console.error(`❌ Index entry validation failed: ${validation.error}`);

        return {
          success: false,
          error: validation.error,
          indexed: false,
          context: this.context,
          sessionId: params.sessionId
        };
      }

      // Store in database using Commands.execute
      const result = await Commands.execute('data/create', {
        collection: CodeIndexEntity.collection,
        data: entry,
        context: this.context,
        sessionId: params.sessionId
      }) as DataCreateResult<CodeIndexEntity>;

      const durationMs = Date.now() - startTime;

      if (!result.success || !result.data) {
        console.error(`❌ Failed to create code index entry: ${result.error || 'Unknown error'}`);

        return {
          success: false,
          error: result.error || 'Failed to create index entry',
          indexed: false,
          context: this.context,
          sessionId: params.sessionId
        };
      }

      console.log(`✅ Created code index entry ${result.data.id} in ${durationMs}ms`);
      if (params.embedding) {
        console.log(`   Stored embedding: ${params.embedding.length} dimensions (${params.embeddingModel})`);
      }

      return {
        success: true,
        entryId: result.data.id,
        indexed: true,
        context: this.context,
        sessionId: params.sessionId
      };
    } catch (error) {
      const durationMs = Date.now() - startTime;

      console.error(`❌ Index entry creation failed after ${durationMs}ms`);
      console.error(`   Error: ${error instanceof Error ? error.message : String(error)}`);

      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        indexed: false,
        context: this.context,
        sessionId: params.sessionId
      };
    }
  }
}
