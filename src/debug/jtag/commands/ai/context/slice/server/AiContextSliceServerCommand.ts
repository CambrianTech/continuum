/**
 * Ai Context Slice Command - Server Implementation
 *
 * Retrieve full content of a context item by ID.
 * Use after context/search to get full content of interesting items.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { AiContextSliceParams, AiContextSliceResult, ContextSliceItem, CollectionName } from '../shared/AiContextSliceTypes';
import { createAiContextSliceResultFromParams } from '../shared/AiContextSliceTypes';
import { Commands } from '@system/core/shared/Commands';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { DataReadParams, DataReadResult } from '@commands/data/read/shared/DataReadTypes';
import type { BaseEntity } from '@system/data/entities/BaseEntity';

import { DataRead } from '../../../../data/read/shared/DataReadTypes';
export class AiContextSliceServerCommand extends CommandBase<AiContextSliceParams, AiContextSliceResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/context/slice', context, subpath, commander);
  }

  async execute(params: AiContextSliceParams): Promise<AiContextSliceResult> {
    const startTime = Date.now();

    // Validate required parameters
    if (!params.id || params.id.trim() === '') {
      throw new ValidationError('id', `Missing required parameter 'id'. Provide the entity ID to retrieve.`);
    }

    if (!params.type || params.type.trim() === '') {
      throw new ValidationError('type', `Missing required parameter 'type'. Provide the collection name (e.g., chat_messages, memories, decisions).`);
    }

    const collection = params.type as CollectionName;

    console.debug(`📄 CONTEXT-SLICE: Fetching ${collection}/${params.id}`);

    try {
      // Read the entity
      const result = await DataRead.execute({
        collection,
        id: params.id
      });

      if (!result.success || !result.data) {
        throw new Error(result.error || 'Entity not found');
      }

      const item = this.mapToSliceItem(result.data, collection);

      // Optionally fetch related items
      let related: ContextSliceItem[] | undefined;
      if (params.includeRelated) {
        related = await this.fetchRelated(collection, result.data, params.relatedLimit || 5);
      }

      const durationMs = Date.now() - startTime;
      console.debug(`✅ CONTEXT-SLICE: Retrieved ${collection}/${params.id} in ${durationMs}ms`);

      return createAiContextSliceResultFromParams(params, {
        success: true,
        item: {
          ...item,
          related
        },
        durationMs
      });

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ CONTEXT-SLICE: Failed:`, errorMessage);
      throw new Error(`Context slice failed: ${errorMessage}`);
    }
  }

  private mapToSliceItem(data: any, collection: CollectionName): ContextSliceItem {
    // Generic content extraction - full content, not truncated
    const content = this.extractFullContent(data);
    const source = this.extractSource(data, collection);

    return {
      id: data.id,
      collection,
      content,
      timestamp: data.timestamp || data.createdAt || new Date().toISOString(),
      source,
      metadata: data.metadata || data
    };
  }

  /**
   * Extract full content from entity - tries common field patterns
   */
  private extractFullContent(data: any): string {
    // Try common content field patterns
    if (data.content?.text) return data.content.text;
    if (typeof data.content === 'string') return data.content;
    if (data.text) return data.text;
    if (data.description) return data.description;
    if (data.message) return data.message;
    if (data.body) return data.body;

    // For tool results or complex data, stringify nicely
    if (data.metadata?.toolName) {
      return JSON.stringify({
        tool: data.metadata.toolName,
        input: data.metadata.toolParams,
        output: data.metadata.fullData || data.content?.text
      }, null, 2);
    }

    // Fallback: stringify the whole thing (excluding base fields)
    const { id, createdAt, updatedAt, version, ...rest } = data;
    return JSON.stringify(rest, null, 2);
  }

  /**
   * Extract source/context info from entity
   */
  private extractSource(data: any, collection: CollectionName): string {
    if (data.roomId) return `Room: ${data.roomId}`;
    if (data.contextId) return `Context: ${data.contextId}`;
    if (data.contextName) return data.contextName;
    if (data.type) return `${collection}: ${data.type}`;
    if (data.category) return `${collection}: ${data.category}`;
    return collection;
  }

  private async fetchRelated(
    collection: CollectionName,
    data: any,
    limit: number
  ): Promise<ContextSliceItem[]> {
    const related: ContextSliceItem[] = [];

    try {
      // Generic related item fetching based on common patterns

      // Thread context (replyTo field)
      if (data.replyTo) {
        const parentResult = await DataRead.execute({
          collection,
          id: data.replyTo
        });
        if (parentResult.success && parentResult.data) {
          related.push(this.mapToSliceItem(parentResult.data, collection));
        }
      }

      // Related IDs array (relatedEventIds, linkedMemoryIds, etc)
      const relatedIdsField = Object.keys(data).find(k =>
        k.endsWith('Ids') && Array.isArray(data[k]) && k !== 'id'
      );
      if (relatedIdsField && data[relatedIdsField].length > 0) {
        for (const relatedId of data[relatedIdsField].slice(0, limit)) {
          const relatedResult = await DataRead.execute({
            collection,
            id: relatedId
          });
          if (relatedResult.success && relatedResult.data) {
            related.push(this.mapToSliceItem(relatedResult.data, collection));
          }
        }
      }
    } catch (error) {
      console.warn(`Failed to fetch related items: ${error}`);
    }

    return related;
  }
}
