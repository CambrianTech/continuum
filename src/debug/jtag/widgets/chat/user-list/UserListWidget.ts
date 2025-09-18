/**
 * User List Widget - Database-Driven Chat Users
 * Properly located in chat directory structure
 */

import { ChatWidgetBase } from '../shared/ChatWidgetBase';
import { JTAGClient } from '../../../system/core/client/shared/JTAGClient';
import type { BaseUser, UserType } from '../../../api/types/User';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { DataRecord } from '../../../daemons/data-daemon/shared/DataStorageAdapter';
import { COLLECTIONS } from '../../../api/data-seed/SeedConstants';

export class UserListWidget extends ChatWidgetBase {
  private users: BaseUser[] = [];

  constructor() {
    super({
      widgetName: 'UserListWidget',
      template: 'user-list-widget.html',
      styles: 'user-list.css',
      enableAI: false,
      enableDatabase: true,
      enableRouterEvents: false,
      enableScreenshots: false
    });
  }

  protected override resolveResourcePath(filename: string): string {
      // Extract widget directory name from widget name (ChatWidget -> chat)
      //const widgetDir = this.config.widgetName.toLowerCase().replace('widget', '');
      // Return relative path from current working directory
      return `widgets/chat/user-list/${filename}`;
    }

  async onWidgetInitialize(): Promise<void> {
    await this.loadUsersFromDatabase();
  }

  private async loadUsersFromDatabase(): Promise<void> {
    const client = await JTAGClient.sharedInstance;
    const result = await this.executeCommand<DataListParams, DataListResult<any>>('data/list', {
      context: client.context,
      sessionId: client.sessionId,
      collection: COLLECTIONS.USERS,
      orderBy: [{ field: 'lastActiveAt', direction: 'desc' }],
      limit: 100
    });

    if (!result?.success || !result.items?.length) {
      console.error('❌ UserListWidget: No users found - check seeding');
      this.users = [];
      return;
    }

    // Convert unified users table data to BaseUser interface
    this.users = result.items
      .filter(user => user?.id && user?.profile?.displayName)
      .map(user => ({
        id: user.id,
        name: user.profile.displayName,
        userType: user.type === 'ai' ? 'agent' : user.type as UserType,
        isAuthenticated: user.status === 'online',
        permissions: [],
        capabilities: Object.entries(user.capabilities || {})
          .filter(([_, enabled]) => enabled)
          .map(([name, _]) => ({ name, enabled: true })),
        metadata: { profile: user.profile, preferences: user.preferences },
        createdAt: user.createdAt,
        lastActiveAt: user.lastActiveAt
      } as BaseUser));

    console.log(`✅ UserListWidget: Loaded ${this.users.length} users from database`);
    console.log(`   Users: ${this.users.map(u => u.name).join(', ')}`);
  }

  protected override async renderWidget(): Promise<void> {
    // Use parent's getReplacements pattern like other widgets
    await super.renderWidget();
  }

  protected async onWidgetCleanup(): Promise<void> {
    this.users = [];
  }

  protected override getReplacements(): Record<string, string> {
    return {
      '<!-- USER_LIST_CONTENT -->': this.renderUserListHTML()
    };
  }

  private renderUserListHTML(): string {
    return this.users.map(user => this.renderUserItem(user)).join('');
  }

  private renderUserItem(user: BaseUser): string {
    const statusClass = user.isAuthenticated ? 'online' : 'offline';
    const avatar = user.userType === 'human' ? '👤' : '🤖';
    const displayName = user.userType === 'human' && user.name === 'human' ? 'joel' : user.name;
    
    return `
      <div class="user-item ${statusClass}" data-user-id="${user.id}">
        <span class="user-avatar">${avatar}</span>
        <div class="user-info">
          <div class="user-name">${displayName}</div>
          <div class="user-type">${user.userType}</div>
        </div>
        <div class="user-status">
          <span class="status-indicator"></span>
        </div>
      </div>
    `;
  }
}