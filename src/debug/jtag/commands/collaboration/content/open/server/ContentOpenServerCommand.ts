/**
 * Content Open Command - Server Implementation
 *
 * Opens content and adds it to user's openItems array in UserStateEntity.
 * Emits content:opened event for widgets to respond to.
 */

import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { ContentOpenParams, ContentOpenResult, ContentOpenedEvent } from '../shared/ContentOpenTypes';
import { ContentOpenCommand } from '../shared/ContentOpenCommand';
import { Commands } from '@system/core/shared/Commands';
import { UserStateEntity } from '@system/data/entities/UserStateEntity';
import { Events } from '@system/core/shared/Events';
import { generateUUID, type UUID } from '@system/core/types/CrossPlatformUUID';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';
import { RoutingService } from '@system/routing/RoutingService';

export class ContentOpenServerCommand extends ContentOpenCommand {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/content/open', context, subpath, commander);
  }

  protected async executeContentCommand(params: ContentOpenParams): Promise<ContentOpenResult> {
    try {
      // userId is REQUIRED in params (should be injected by infrastructure from session)
      const userId = params.userId;

      // 1. Load user's UserStateEntity from database
      const listResult = await Commands.execute<DataListParams, DataListResult<UserStateEntity>>(DATA_COMMANDS.LIST, {
        collection: 'user_states',
        filter: { userId },
        limit: 1
      });

      if (!listResult?.success || !listResult.items || listResult.items.length === 0) {
        return transformPayload(params, {
          success: false,
          contentItemId: '' as UUID,
          openItemsCount: 0,
          error: `No UserState found for userId: ${userId}`
        });
      }

      const userState = Object.assign(new UserStateEntity(), listResult.items[0]);

      // 2. Resolve entityId to canonical UUID, uniqueId, and display name
      //    URL might use "general" but DB stores UUID "5e71a0c8-..."
      let canonicalEntityId = params.entityId;
      let resolvedUniqueId: string | undefined;
      let resolvedDisplayName: string | undefined;
      if (params.entityId) {
        const resolved = await RoutingService.resolve(params.contentType, params.entityId);
        if (resolved) {
          canonicalEntityId = resolved.id;
          resolvedUniqueId = resolved.uniqueId;
          resolvedDisplayName = resolved.displayName;
        }
      }

      // 3. Generate unique ID for the content item
      const contentItemId = generateUUID();

      // 4. Derive title: prefer params.title, then resolved displayName, then fallback
      const title = params.title || resolvedDisplayName || this.deriveTitle(params.contentType);

      // 5. Add content item using UserStateEntity method (uses canonical UUID for deduplication)
      userState.addContentItem({
        id: contentItemId,
        type: params.contentType,
        entityId: canonicalEntityId,
        uniqueId: resolvedUniqueId,  // Store for fast URL building without async resolution
        title,
        subtitle: params.subtitle,
        priority: params.priority || 'normal',
        metadata: params.metadata
      });

      // 6. Optionally set as current item (default: true)
      const setAsCurrent = params.setAsCurrent !== false; // Default to true
      if (setAsCurrent) {
        userState.setCurrentContent(contentItemId);
      }

      // 7. Save updated userState to database
      await Commands.execute<DataUpdateParams, DataUpdateResult>(DATA_COMMANDS.UPDATE, {
        collection: 'user_states',
        id: userState.id,
        data: userState
      });

      // 8. Emit content:opened event for widgets to respond to
      //    Include both UUID (for DB) and uniqueId (for URLs)
      const event: ContentOpenedEvent = {
        contentItemId,
        contentType: params.contentType,
        entityId: canonicalEntityId,  // UUID for database lookups
        uniqueId: resolvedUniqueId,   // Human-readable for URLs (e.g., "general")
        title,
        userId,
        currentItemId: userState.contentState.currentItemId,
        setAsCurrent
      };

      await Events.emit(this.context, 'content:opened', event);

      // 9. Return success result
      return transformPayload(params, {
        success: true,
        contentItemId,
        currentItemId: userState.contentState.currentItemId,
        openItemsCount: userState.contentState.openItems.length
      });

    } catch (error) {
      console.error('❌ ContentOpenServerCommand: Error opening content:', error);
      return transformPayload(params, {
        success: false,
        contentItemId: '' as UUID,
        openItemsCount: 0,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }

  /**
   * Derive a display title from contentType for singleton content
   * e.g., 'settings' -> 'Settings', 'user-profile' -> 'User Profile'
   */
  private deriveTitle(contentType: string): string {
    // Handle hyphenated types like 'user-profile' -> 'User Profile'
    return contentType
      .split('-')
      .map(word => word.charAt(0).toUpperCase() + word.slice(1))
      .join(' ');
  }
}
