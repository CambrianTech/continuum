/**
 * Typing Indicator - Manages ephemeral "user is typing..." display
 *
 * Follows the proven AIStatusIndicator pattern:
 * - Map-based state tracking per user
 * - Auto-decay timers (3 seconds)
 * - Container-based DOM rendering
 * - Room-scoped filtering
 */

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { TypingEventPayload } from '@system/core/shared/EventConstants';

/** How long before a typing indicator auto-expires (ms) */
const TYPING_DECAY_MS = 3000;

/**
 * Active typer state
 */
interface TypingState {
  userId: UUID;
  displayName: string;
  roomId: UUID;
  timestamp: number;
  decayTimer: ReturnType<typeof setTimeout>;
}

/**
 * Typing Indicator Manager
 * Renders "Name is typing..." below messages, above input
 */
export class TypingIndicator {
  private _activeTypers = new Map<UUID, TypingState>();
  private _container?: HTMLElement;
  private _roomId?: UUID;

  /**
   * Set the DOM container for rendering typing text
   */
  setContainer(container: HTMLElement): void {
    this._container = container;
  }

  /**
   * Set the current room — only shows typing from this room
   */
  setRoomId(roomId: UUID): void {
    this._roomId = roomId;
  }

  /**
   * Handle typing start event
   */
  onTypingStart(data: TypingEventPayload): void {
    // Filter to current room
    if (data.roomId !== this._roomId) return;

    const userId = data.userId as UUID;
    const existing = this._activeTypers.get(userId);

    // Clear existing decay timer
    if (existing) {
      clearTimeout(existing.decayTimer);
    }

    // Set new decay timer
    const decayTimer = setTimeout(() => {
      this._activeTypers.delete(userId);
      this.updateDisplay();
    }, TYPING_DECAY_MS);

    this._activeTypers.set(userId, {
      userId,
      displayName: data.displayName,
      roomId: data.roomId as UUID,
      timestamp: Date.now(),
      decayTimer,
    });

    this.updateDisplay();
  }

  /**
   * Handle typing stop event
   */
  onTypingStop(data: TypingEventPayload): void {
    if (data.roomId !== this._roomId) return;

    const userId = data.userId as UUID;
    const existing = this._activeTypers.get(userId);
    if (existing) {
      clearTimeout(existing.decayTimer);
      this._activeTypers.delete(userId);
      this.updateDisplay();
    }
  }

  /**
   * Clear all typing indicators (e.g., room switch)
   */
  clearAll(): void {
    for (const state of this._activeTypers.values()) {
      clearTimeout(state.decayTimer);
    }
    this._activeTypers.clear();
    this.updateDisplay();
  }

  /**
   * Update the DOM container with current typing state
   */
  private updateDisplay(): void {
    if (!this._container) return;

    const typers = Array.from(this._activeTypers.values());

    if (typers.length === 0) {
      this._container.innerHTML = '';
      return;
    }

    const names = typers.map(t => t.displayName);
    let text: string;

    if (names.length === 1) {
      text = `${names[0]} is typing...`;
    } else if (names.length === 2) {
      text = `${names[0]} and ${names[1]} are typing...`;
    } else {
      text = `${names[0]} and ${names.length - 1} others are typing...`;
    }

    this._container.innerHTML = `<span class="typing-text">${text}</span>`;
  }

  /**
   * Get count of active typers (for testing/debugging)
   */
  get activeCount(): number {
    return this._activeTypers.size;
  }
}
