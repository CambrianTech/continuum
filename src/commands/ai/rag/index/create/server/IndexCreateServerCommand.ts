/**
 * Index Create Server Command
 *
 * Server-side implementation for storing code index entries
 */

import { IndexCreateCommand } from '../shared/IndexCreateCommand';
import type { JTAGContext } from '../../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../../daemons/command-daemon/shared/CommandBase';
import type { IndexCreateParams, IndexCreateResult } from '../shared/IndexCreateTypes';
import type { DataCreateParams, DataCreateResult } from '../../../../../data/create/shared/DataCreateTypes';
import type { EmbeddingGenerateResult } from '../../../../embedding/generate/shared/EmbeddingGenerateTypes';
import { CodeIndexEntity } from '../../../../../../system/data/entities/CodeIndexEntity';
import { Commands } from '../../../../../../system/core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';

import { EmbeddingGenerate } from '../../../../embedding/generate/shared/EmbeddingGenerateTypes';
import { DataCreate } from '../../../../../data/create/shared/DataCreateTypes';
export class IndexCreateServerCommand extends IndexCreateCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/rag/index/create', context, subpath, commander);
  }

  async execute(params: IndexCreateParams): Promise<IndexCreateResult> {
    const startTime = Date.now();

    try {
      console.log(`📇 Creating code index entry for ${params.filePath}`);

      // Generate embedding if not provided
      let embedding = params.embedding;
      const embeddingModel = params.embeddingModel || 'nomic-embed-text';

      if (!embedding) {
        console.log(`🧬 Generating embedding for content (${params.content.length} chars)`);
        const embeddingResult = await EmbeddingGenerate.execute({
          input: params.content,
          model: embeddingModel,
          context: this.context,
          sessionId: params.sessionId
        }) as EmbeddingGenerateResult;

        if (!embeddingResult.success || !embeddingResult.embeddings || embeddingResult.embeddings.length === 0) {
          console.error(`❌ Failed to generate embedding: ${embeddingResult.error || 'Unknown error'}`);
          return {
            success: false,
            error: `Failed to generate embedding: ${embeddingResult.error || 'Unknown error'}`,
            indexed: false,
            context: this.context,
            sessionId: params.sessionId
          };
        }

        embedding = embeddingResult.embeddings[0];
        console.log(`✅ Generated embedding: ${embedding.length} dimensions`);
      }

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
      entry.embedding = embedding;
      entry.embeddingModel = embeddingModel;
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
      const result = await DataCreate.execute({
        collection: CodeIndexEntity.collection,
        data: entry,
        context: this.context,
        sessionId: params.sessionId
      });

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
      if (embedding) {
        console.log(`   Stored embedding: ${embedding.length} dimensions (${embeddingModel})`);
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
