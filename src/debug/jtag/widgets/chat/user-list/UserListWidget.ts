/**
 * User List Widget - Database-Driven Chat Users
 * Now uses EntityScrollerWidget base class for automatic EntityScroller management
 */

import { EntityScrollerWidget } from '../../shared/EntityScrollerWidget';
import { UserEntity } from '../../../system/data/entities/UserEntity';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import { Commands } from '../../../system/core/client/shared/Commands';
import { SCROLLER_PRESETS, type RenderFn, type LoadFn, type ScrollerConfig } from '../../shared/EntityScroller';

export class UserListWidget extends EntityScrollerWidget<UserEntity> {

  constructor() {
    super({
      widgetId: 'user-list-widget',
      widgetName: 'UserListWidget',
      styles: 'user-list.css',
      enableAI: false,
      enableDatabase: true,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  // Path resolution now handled automatically by ChatWidgetBase
  // EntityScroller setup now handled automatically by EntityScrollerWidget base class

  // Required by EntityScrollerWidget - render function for individual user items
  protected getRenderFunction(): RenderFn<UserEntity> {
    return (user: UserEntity, _context) => {
      const statusClass = user.status === 'online' ? 'online' : 'offline';

      // ✨ BEAUTIFUL: Intelligent avatar defaults based on user type
      const avatar = user.profile?.visualIdentity?.avatar ??
        (user.type === 'human' ? '👤' :
         user.type === 'agent' ? '🤖' :
         user.type === 'persona' ? '⭐' :
         user.type === 'system' ? '⚙️' : '❓');

      const speciality = user.profile?.speciality ?? null;
      const displayName = user.displayName ?? 'Unknown User';

      const userElement = globalThis.document.createElement('div');
      userElement.className = `user-item ${statusClass}`;
      userElement.innerHTML = `
        <span class="user-avatar">${avatar}</span>
        <div class="user-info">
          <div class="user-name">${displayName}</div>
          <div class="user-meta">
            <span class="user-type">${user.type}</span>
            ${speciality ? `<span class="user-speciality">${speciality}</span>` : ''}
          </div>
        </div>
        <div class="user-status">
          <span class="status-indicator"></span>
        </div>
      `;

      return userElement;
    };
  }

  // Required by EntityScrollerWidget - load function using data/list command
  protected getLoadFunction(): LoadFn<UserEntity> {
    return async (cursor, limit) => {
      const result = await Commands.execute<DataListParams<UserEntity>, DataListResult<UserEntity>>('data/list', {
        collection: UserEntity.collection,
        orderBy: [{ field: 'lastActiveAt', direction: 'desc' }],
        limit: limit ?? 100
      });

      if (!result?.success || !result.items) {
        throw new Error(`Failed to load users: ${result?.error ?? 'Unknown error'}`);
      }

      return {
        items: result.items,
        hasMore: false, // User lists are typically small, no pagination needed
        nextCursor: undefined
      };
    };
  }

  // Required by EntityScrollerWidget
  protected getScrollerPreset(): ScrollerConfig {
    return SCROLLER_PRESETS.LIST; // No auto-scroll, larger page size
  }

  // Required by EntityScrollerWidget
  protected getContainerSelector(): string {
    return '.user-list';
  }

  // Required by EntityScrollerWidget
  protected getEntityCollection(): string {
    return UserEntity.collection;
  }

  protected resolveResourcePath(filename: string): string {
    return `widgets/chat/user-list/${filename}`;
  }

  // Event subscriptions now handled automatically by EntityScrollerWidget base class


  // Entity count now handled automatically by EntityScrollerWidget base class

  protected getEntityTitle(_entity?: UserEntity): string {
    return 'Users & Agents';
  }

  // Cleanup now handled automatically by EntityScrollerWidget base class

  // Using default template from EntityScrollerWidget (generates "user-list" CSS class automatically)

}