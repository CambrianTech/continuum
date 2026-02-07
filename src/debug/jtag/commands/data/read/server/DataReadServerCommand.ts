/**
 * Data Read Command - Server Implementation
 *
 * Updated to use DataDaemon for consistent storage access
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataReadParams, DataReadResult } from '../shared/DataReadTypes';
import { createDataReadResultFromParams } from '../shared/DataReadTypes';
import { ORM } from '../../../../daemons/data-daemon/shared/ORM';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import type { MediaItem, ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';
import { DataReadCommand } from '../shared/DataReadCommand';
import { isValidCollection, type CollectionName, COLLECTIONS } from '../../../../shared/generated-collection-constants';

export class DataReadServerCommand extends DataReadCommand<BaseEntity> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-read', context, subpath, commander);
  }

  protected async executeDataCommand(params: DataReadParams): Promise<DataReadResult<BaseEntity>> {
    // Validate required parameters
    if (!params.collection) {
      return createDataReadResultFromParams(params, {
        success: false,
        error: 'Missing required parameter: collection. Use data/list to see available collections.',
        found: false
      });
    }

    if (!params.id) {
      return createDataReadResultFromParams(params, {
        success: false,
        error: 'Missing required parameter: id',
        found: false
      });
    }

    // Validate collection name at runtime (user input comes as string)
    if (!isValidCollection(params.collection)) {
      return createDataReadResultFromParams(params, {
        success: false,
        error: `Invalid collection name: ${params.collection}`,
        found: false
      });
    }
    const validCollection = params.collection as CollectionName;

    try {
      // Use DataDaemon for consistent storage access
      const entity = await ORM.read<BaseEntity>(validCollection, params.id);

      if (entity) {

        // Extract media if this is a chat message with attachments
        let media: MediaItem[] = [];
        let cleanedData: BaseEntity = entity;

        if (validCollection === COLLECTIONS.CHAT_MESSAGES) {
          const messageData = entity as ChatMessageEntity;
          if (messageData.content?.media && Array.isArray(messageData.content.media)) {
            // Extract media to top level
            media = messageData.content.media;

            // Create cleaned entity without media duplication (preserve prototype)
            cleanedData = Object.assign(
              Object.create(Object.getPrototypeOf(messageData)),
              messageData,
              {
                content: {
                  ...messageData.content,
                  media: []
                }
              }
            );
          }
        }

        return createDataReadResultFromParams(params, {
          success: true,
          data: cleanedData,
          found: true,
          media
        });
      } else {

        return createDataReadResultFromParams(params, {
          success: true,
          data: undefined,
          found: false
        });
      }
    } catch (error) {
      console.error(`❌ DATA SERVER: Failed to read ${params.collection}/${params.id}:`, error);

      return createDataReadResultFromParams(params, {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
        found: false
      });
    }
  }
}