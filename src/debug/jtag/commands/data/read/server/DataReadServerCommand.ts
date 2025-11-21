/**
 * Data Read Command - Server Implementation
 *
 * Updated to use DataDaemon for consistent storage access
 */

import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataReadParams, DataReadResult } from '../shared/DataReadTypes';
import { createDataReadResultFromParams } from '../shared/DataReadTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { BaseEntity } from '../../../../system/data/entities/BaseEntity';
import { DataReadCommand } from '../shared/DataReadCommand';
import { extractMediaFromMessage, isChatMessageEntity } from '../../shared/ChatMessageTransforms';

export class DataReadServerCommand extends DataReadCommand<BaseEntity> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-read', context, subpath, commander);
  }

  protected async executeDataCommand(params: DataReadParams): Promise<DataReadResult<BaseEntity>> {
    console.log(`🗄️ DATA SERVER: Reading ${params.collection}/${params.id} via DataDaemon`);

    try {
      // Use DataDaemon for consistent storage access
      const result = await DataDaemon.read<BaseEntity>(params.collection, params.id);

      if (result.success && result.data) {
        console.log(`✅ DATA SERVER: Read ${params.collection}/${params.id}`);

        // Extract media from chat messages to avoid duplication in response
        const transformation = isChatMessageEntity(params.collection, result.data.data)
          ? extractMediaFromMessage(result.data.data)
          : { entity: result.data.data, media: [] };

        if (transformation.media.length > 0) {
          console.log(`📸 DATA SERVER: Extracted ${transformation.media.length} media item(s) from message ${params.id}`);
        }

        return createDataReadResultFromParams(params, {
          success: true,
          data: transformation.entity,
          found: true,
          media: transformation.media
        });
      } else {
        console.log(`ℹ️ DATA SERVER: Record not found ${params.collection}/${params.id}`);

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