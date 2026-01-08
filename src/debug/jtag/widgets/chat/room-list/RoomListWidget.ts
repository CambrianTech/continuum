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
import type { ContentOpenParams, ContentOpenResult } from '../../../commands/collaboration/content/open/shared/ContentOpenTypes';
import { RoomEntity } from '../../../system/data/entities/RoomEntity';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { Commands } from '../../../system/core/shared/Commands';
import { Events } from '../../../system/core/shared/Events';
import { UI_EVENTS } from '../../../system/core/shared/EventConstants';
import { DEFAULT_ROOMS } from '../../../system/data/domains/DefaultEntities';

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
  protected override shouldAddEntity(room: RoomEntity): boolean {
    return !(room.tags ?? []).includes('system');
  }

  // === LIFECYCLE ===
  protected override onFirstRender(): void {
    super.onFirstRender();

    this.createMountEffect(() => {
      const unsubscribe = Events.subscribe(
        UI_EVENTS.ROOM_SELECTED,
        (data: { roomId: string }) => {
          const newRoomId = data.roomId as UUID;
          if (newRoomId !== this.currentRoomId) {
            this.currentRoomId = newRoomId;
          }
        }
      );
      return () => unsubscribe();
    });
  }

  // === ROOM SELECTION ===
  private selectRoom(room: RoomEntity): void {
    const roomId = room.id as UUID;
    if (this.currentRoomId === roomId) return;

    this.currentRoomId = roomId;

    Events.emit(UI_EVENTS.ROOM_SELECTED, {
      roomId,
      roomName: room.displayName || room.name,
      uniqueId: room.uniqueId || room.name || roomId
    });

    const userId = this.currentUser?.id;
    if (userId) {
      Commands.execute<ContentOpenParams, ContentOpenResult>('collaboration/content/open', {
        userId,
        contentType: 'chat',
        entityId: roomId,
        title: room.displayName || room.name,
        subtitle: room.topic,
        setAsCurrent: true
      }).catch(err => console.error('Failed to persist room open:', err));
    }
  }

  protected override onItemClick(_item: RoomEntity): void {
    // Handled by @click in renderItem template
  }
}
