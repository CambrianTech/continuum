/**
 * Room List Widget - Uses ReactiveListWidget with header/item/footer pattern
 */

import {
  ReactiveListWidget,
  html,
  reactive,
  unsafeCSS,
  type TemplateResult,
  type CSSResultGroup
} from '../../shared/ReactiveListWidget';
import { RoomEntity } from '../../../system/data/entities/RoomEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { DEFAULT_ROOMS } from '../../../system/data/domains/DefaultEntities';
import { pageState } from '../../../system/state/PageStateService';
import { ContentService } from '../../../system/state/ContentService';

import { styles as externalStyles } from './room-list-widget.styles';

export class RoomListWidget extends ReactiveListWidget<RoomEntity> {
  readonly collection = RoomEntity.collection;

  @reactive() private currentRoomId: UUID = DEFAULT_ROOMS.GENERAL as UUID;

  static override styles = [
    ReactiveListWidget.styles,
    unsafeCSS(externalStyles)
  ] as CSSResultGroup;

  constructor() {
    super({ widgetName: 'RoomListWidget' });
  }

  // === CONFIGURATION ===
  protected override get listTitle(): string { return 'Rooms'; }
  protected override get containerClass(): string { return 'entity-list-body'; }

  // === HEADER (uses SCSS classes) ===
  protected override renderHeader(): TemplateResult {
    return html`
      <div class="entity-list-header">
        <span class="list-title">${this.listTitle}</span>
        <span class="list-count">${this.entityCount}</span>
      </div>
    `;
  }

  // === ITEM ===
  renderItem(room: RoomEntity): TemplateResult {
    const isActive = room.id === this.currentRoomId;
    return html`
      <div class="room-item ${isActive ? 'active' : ''}"
           data-room-id="${room.id}"
           @click=${() => this.selectRoom(room)}>
        <div class="room-info">
          <div class="room-name">${room.displayName ?? room.name}</div>
          <div class="room-topic">${room.topic ?? ''}</div>
        </div>
      </div>
    `;
  }

  // === MAIN RENDER (uses SCSS container) ===
  override render(): TemplateResult {
    return html`
      <div class="entity-list-container">
        ${this.renderHeader()}
        <div class="${this.containerClass}"></div>
        ${this.renderFooter()}
      </div>
    `;
  }

  // === FILTERING ===

  /** Filter out system rooms and DMs from main room list */
  protected override shouldAddEntity(room: RoomEntity): boolean {
    // Hide system rooms (settings, help, theme, canvas sidebars)
    if ((room.tags ?? []).includes('system')) return false;
    // Hide DM/private rooms
    if (room.type === 'direct') return false;
    if ((room.tags ?? []).includes('dm')) return false;
    return true;
  }

  // === LIFECYCLE ===
  protected override onFirstRender(): void {
    super.onFirstRender();

    // Subscribe to pageState - single source of truth for current room
    this.createMountEffect(() => {
      const unsubscribe = pageState.subscribe((state) => {
        if (state.contentType === 'chat' && state.entityId) {
          // entityId might be UUID or uniqueId - find matching room
          const matchingRoom = this.entities.find(
            (room: RoomEntity) => room.id === state.entityId || room.uniqueId === state.entityId
          );
          const newRoomId = matchingRoom?.id as UUID || state.entityId as UUID;
          if (newRoomId !== this.currentRoomId) {
            this.currentRoomId = newRoomId;
            // Scroll handled by updated() lifecycle
          }
        }
      });
      return () => unsubscribe();
    });
  }

  // === REACTIVE SCROLL - triggers on @reactive() currentRoomId change ===
  protected override updated(changedProperties: Map<string, unknown>): void {
    super.updated(changedProperties);

    if (changedProperties.has('currentRoomId')) {
      // Scroll selected room into view with smooth animation
      // Use multiple rAF to ensure DOM is fully updated
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          const container = this.shadowRoot?.querySelector('.entity-list-body') as HTMLElement;
          const activeItem = this.shadowRoot?.querySelector('.room-item.active') as HTMLElement;
          if (container && activeItem) {
            // Scroll item into view within the container
            const containerRect = container.getBoundingClientRect();
            const itemRect = activeItem.getBoundingClientRect();

            // Check if item is outside visible area
            if (itemRect.top < containerRect.top || itemRect.bottom > containerRect.bottom) {
              activeItem.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
            }
          }
        });
      });
    }
  }

  // === ROOM SELECTION ===
  private selectRoom(room: RoomEntity): void {
    const roomId = room.id as UUID;
    if (this.currentRoomId === roomId) return;

    this.currentRoomId = roomId;

    // Set userId on service if not already set
    if (this.currentUser?.id) {
      ContentService.setUserId(this.currentUser.id as UUID);
    }

    // ONE call handles: tab, view, URL, persist
    ContentService.open('chat', roomId, {
      title: room.displayName || room.name,
      subtitle: room.topic,
      uniqueId: room.uniqueId || room.name || roomId
    });
  }

  protected override onItemClick(_item: RoomEntity): void {
    // Handled by @click in renderItem template
  }
}
