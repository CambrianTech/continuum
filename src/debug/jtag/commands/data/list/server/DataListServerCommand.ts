/**
 * Data List Command - Server Implementation
 * 
 * Uses global database storage following ArtifactsDaemon database storage pattern
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DataListParams, DataListResult } from '../shared/DataListTypes';
import { createDataListResultFromParams } from '../shared/DataListTypes';
import { WorkingDirConfig } from '../../../../system/core/config/WorkingDirConfig';
import type { ChatMessageData } from '../../../../domain/chat/ChatMessage';

// Database record structure from data storage system
interface DatabaseRecord<T> {
  readonly id: UUID;
  readonly collection: string;
  readonly data: T;
  readonly metadata: {
    readonly createdAt: string;
    readonly updatedAt: string;
    readonly version: number;
  };
}

// Type for chat message database records
type ChatMessageRecord = DatabaseRecord<ChatMessageData>;

// Rust-style config defaults with high values for chat applications
const DEFAULT_CONFIG = {
  database: {
    queryLimit: 100,      // High default for substantial context
    maxBatchSize: 500,    // Safety ceiling
    minLimit: 1,          // Never allow 0 or negative
  }
} as const;


export class DataListServerCommand<T> extends CommandBase<DataListParams, DataListResult<T>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-list', context, subpath, commander);
  }

  /**
   * Type-safe field value extraction from database records
   */
  private getFieldValue(record: DatabaseRecord<unknown>, field: string): unknown {
    if (field.startsWith('metadata.')) {
      const metadataField = field.substring(9) as keyof DatabaseRecord<unknown>['metadata'];
      return record.metadata[metadataField];
    }
    if (field.startsWith('data.')) {
      const dataField = field.substring(5);
      return (record.data as Record<string, unknown>)[dataField];
    }
    return (record as unknown as Record<string, unknown>)[field];
  }

  /**
   * Type-safe comparison of unknown values with proper timestamp handling
   */
  private compareValues(a: unknown, b: unknown): number {
    // Handle null/undefined cases
    if (a == null && b == null) return 0;
    if (a == null) return -1;
    if (b == null) return 1;

    // Handle date/timestamp comparison properly
    const aStr = String(a);
    const bStr = String(b);

    // Check if values look like ISO timestamps
    const isADate = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/.test(aStr);
    const isBDate = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/.test(bStr);

    if (isADate && isBDate) {
      const aTime = new Date(aStr).getTime();
      const bTime = new Date(bStr).getTime();
      return aTime - bTime;
    }

    // Numeric comparison for numbers
    const aNum = Number(a);
    const bNum = Number(b);
    if (!isNaN(aNum) && !isNaN(bNum)) {
      return aNum - bNum;
    }

    // String comparison fallback
    if (aStr < bStr) return -1;
    if (aStr > bStr) return 1;
    return 0;
  }

  /**
   * Apply proper orderBy sorting to items using DataListParams structure
   */
  private applySorting<T>(items: T[], orderBy: Array<{ field: string; direction: 'asc' | 'desc' }>): void {
    items.sort((a, b) => {
      for (const sortSpec of orderBy) {
        const aRecord = a as DatabaseRecord<unknown>;
        const bRecord = b as DatabaseRecord<unknown>;

        const aValue = this.getFieldValue(aRecord, sortSpec.field);
        const bValue = this.getFieldValue(bRecord, sortSpec.field);

        const comparison = this.compareValues(aValue, bValue);

        if (comparison !== 0) {
          return sortSpec.direction === 'desc' ? -comparison : comparison;
        }
      }
      return 0;
    });
  }

  async execute(params: DataListParams): Promise<DataListResult<T>> {
    console.debug(`🗄️ DATA SERVER: Listing ${params.collection}`);

    try {
      // Use global database path following DataCreateServerCommand pattern
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const databasePath = path.join(continuumPath, 'database');
      const collectionPath = path.join(databasePath, params.collection);

      // Check if collection directory exists
      try {
        await fs.access(collectionPath);
      } catch {
        console.debug(`ℹ️ DATA SERVER: Collection ${params.collection} not found`);
        return createDataListResultFromParams(params, {
          success: true,
          items: [],
          count: 0
        });
      }

      // Read all files from the collection directory
      const files = await fs.readdir(collectionPath);
      const jsonFiles = files.filter(file => file.endsWith('.json'));

      const items: T[] = [];
      const limit = Math.min(params.limit ?? DEFAULT_CONFIG.database.queryLimit, DEFAULT_CONFIG.database.maxBatchSize);

      // Read ALL files first (before applying limit for proper sorting)
      for (const file of jsonFiles) {
        try {
          const filePath = path.join(collectionPath, file);
          const content = await fs.readFile(filePath, 'utf-8');
          const data = JSON.parse(content);
          items.push(data);
        } catch (error) {
          console.warn(`⚠️ DATA SERVER: Failed to read ${file}:`, error);
        }
      }

      // Apply filtering if provided
      let filteredItems = items;
      if (params.filter && Object.keys(params.filter).length > 0) {
        filteredItems = items.filter(item => {
          for (const [key, value] of Object.entries(params.filter!)) {
            const record = item as DatabaseRecord<unknown>;
            const fieldValue = this.getFieldValue(record, key);
            if (fieldValue !== value) {
              return false;
            }
          }
          return true;
        });
      }

      // For chat messages, we need special handling to get most recent N messages
      // but display them chronologically (oldest to newest, recent at bottom)
      let finalItems: T[];

      if (params.collection === 'chat_messages') {
        // Step 1: Sort ALL messages by timestamp DESC to find most recent ones
        filteredItems.sort((a, b) => {
          const aRecord = a as ChatMessageRecord;
          const bRecord = b as ChatMessageRecord;
          const aTimestamp = new Date(aRecord.metadata.createdAt).getTime();
          const bTimestamp = new Date(bRecord.metadata.createdAt).getTime();
          return bTimestamp - aTimestamp; // DESC - newest first
        });

        // Step 2: Take the most recent N messages
        const recentMessages = filteredItems.slice(0, limit);

        // Step 3: Sort those N messages ASC for display (oldest first, recent at bottom)
        recentMessages.sort((a, b) => {
          const aRecord = a as ChatMessageRecord;
          const bRecord = b as ChatMessageRecord;
          const aTimestamp = new Date(aRecord.metadata.createdAt).getTime();
          const bTimestamp = new Date(bRecord.metadata.createdAt).getTime();
          return aTimestamp - bTimestamp; // ASC - oldest first
        });

        finalItems = recentMessages;
      } else {
        // Generic handling for other collections with proper orderBy support
        if (params.orderBy && params.orderBy.length > 0) {
          this.applySorting(filteredItems, params.orderBy);
        }
        finalItems = filteredItems.slice(0, limit);
      }

      console.debug(`✅ DATA SERVER: Listed ${finalItems.length}/${items.length} items from ${params.collection} (chronologically sorted)`);

      return createDataListResultFromParams(params, {
        success: true,
        items: finalItems,
        count: finalItems.length
      });

    } catch (error: unknown) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ DATA SERVER: List operation failed:`, errorMessage);
      return createDataListResultFromParams(params, {
        success: false,
        items: [],
        count: 0,
        error: errorMessage
      });
    }
  }
}