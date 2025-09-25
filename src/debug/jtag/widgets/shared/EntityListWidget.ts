/**
 * EntityListWidget - Pure extension of ChatWidgetBase
 * Just adds common entity list patterns, nothing fancy
 */

import { ChatWidgetBase } from '../chat/shared/ChatWidgetBase';
import { BaseEntity } from '../../system/data/entities/BaseEntity';

export abstract class EntityListWidget<T extends BaseEntity> extends ChatWidgetBase {

  constructor(config: any) {
    super(config);
  }

  // Standardized header structure with entity-driven content
  protected renderHeader(entity?: T): string {
    return `
      <div class="entity-list-header">
        <span class="header-title">${this.getEntityTitle(entity)}</span>
        <span class="list-count">${this.getEntityCount()}</span>
      </div>
    `;
  }

  // Each widget defines its title based on entity context
  protected abstract getEntityTitle(entity?: T): string;

  // Widgets implement their own count logic:
  // - Room/User: total items (equals loaded since no pagination)
  // - Chat: loaded items (subset of total via infinite scroll)
  protected abstract getEntityCount(): number;

  // Shared count update logic
  protected updateEntityCount(): void {
    const countElement = this.shadowRoot.querySelector('.list-count');
    if (countElement) {
      countElement.textContent = this.getEntityCount().toString();
    }
  }
}