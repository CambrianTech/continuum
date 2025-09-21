/**
 * Data Create Command - Server Implementation
 *
 * Uses DataDaemon for proper storage abstraction (SQLite backend)
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '../shared/DataCreateTypes';
import { createDataCreateResultFromParams } from '../shared/DataCreateTypes';
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import type { BaseEntity } from '../../../../system/data/domains/CoreTypes';
import { UserEntity } from '../../../../system/data/entities/UserEntity';
import { ChatMessageEntity } from '../../../../system/data/entities/ChatMessageEntity';
import { RoomEntity } from '../../../../system/data/entities/RoomEntity';
import { JTAGMessageFactory } from '../../../../system/core/types/JTAGTypes';
import { JTAG_ENDPOINTS } from '../../../../system/core/router/shared/JTAGEndpoints';
import type { EventBridgePayload } from '../../../../daemons/events-daemon/shared/EventsDaemon';
import { EVENT_SCOPES } from '../../../../system/events/shared/EventSystemConstants';
import { getDataEventName } from '../../shared/DataEventConstants';


export class DataCreateServerCommand extends CommandBase<DataCreateParams, DataCreateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-create', context, subpath, commander);
  }


  async execute(params: DataCreateParams): Promise<DataCreateResult> {
    const collection = params.collection;
    console.debug(`🗄️ DATA SERVER: Creating ${collection} record via DataDaemon`);

    try {
      const id = params.id ?? generateUUID();

      // Use enhanced DataDaemon with field extraction
      const result = await DataDaemon.store(collection, params.data, id);

      if (result.success && result.data) {
        console.debug(`✅ DATA SERVER: Created ${collection}/${result.data.id} with field extraction`);
        console.debug(`🔧 CLAUDE-FIX-${Date.now()}: DataCreateServerCommand now uses DataDaemon.store() for field extraction`);

        // Emit generic data creation event for real-time UI updates using EventBridge
        // Entity must match EXACTLY what DataListServerCommand returns
        try {
          console.log(`📡 DataCreateServerCommand: Emitting data:${collection}:created event via EventBridge`);
          // Match DataListServerCommand format: merge record.data + record.id
          const entityData = {
            ...result.data.data,
            id: result.data.id  // Same merge as DataListServerCommand lines 73-76
          };
          await this.emitDataCreatedEvent(collection, entityData);
          console.log(`✅ DataCreateServerCommand: Successfully emitted data:${collection}:created event`);
        } catch (eventError) {
          console.error(`❌ DataCreateServerCommand: Failed to emit event:`, eventError);
          // Don't fail the command if event emission fails
        }

        return createDataCreateResultFromParams(params, {
          success: true,
          id: result.data.id
        });
      } else {
        console.error(`❌ DATA SERVER: DataDaemon.store failed:`, result.error);
        return createDataCreateResultFromParams(params, {
          success: false,
          error: result.error || 'Unknown error'
        });
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ DATA SERVER: DataDaemon create failed:`, errorMessage);
      return createDataCreateResultFromParams(params, {
        success: false,
        error: errorMessage
      });
    }
  }

  /**
   * Server-specific event emission using Router's event facilities (like chat system)
   */
  private async emitDataCreatedEvent(collection: string, data: any): Promise<void> {
    try {
      if (!this.commander?.router) {
        throw new Error('Router not available for event emission');
      }

      // Create EventBridge payload for global data creation event
      const eventPayload: EventBridgePayload = {
        context: this.context,
        sessionId: this.context.uuid,
        type: 'event-bridge',
        scope: {
          type: EVENT_SCOPES.GLOBAL,
          id: '',
          sessionId: this.context.uuid
        },
        eventName: getDataEventName(collection, 'created'),
        data: data, // Just the entity directly
        originSessionId: this.context.uuid,
        originContextUUID: this.context.uuid,
        timestamp: new Date().toISOString()
      };

      // Create event message using JTAG message factory
      const eventMessage = JTAGMessageFactory.createEvent(
        this.context,
        'data-create',
        JTAG_ENDPOINTS.EVENTS.BRIDGE,
        eventPayload
      );

      // Route event through Router (handles cross-context distribution)
      const result = await this.commander.router.postMessage(eventMessage);
      console.log(`📨 SERVER-EVENT: Emitted data:${collection}:created for record ${data.id}`, result);

    } catch (error) {
      console.error(`❌ DataCreateServerCommand: Failed to emit EventBridge event:`, error);
      throw error;
    }
  }
}