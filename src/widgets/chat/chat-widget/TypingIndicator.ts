/**
 * Typing Indicator - Single persistent row: "Helper AI, Teacher AI are typing..."
 *
 * One line. Always reserved. Names accumulate/drop. No layout shifts.
 */

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { TypingEventPayload } from '@system/core/shared/EventConstants';

/** Safety net — TYPING_STOP is the real removal trigger. */
const TYPING_DECAY_MS = 30_000;

interface TypingState {
  displayName: string;
  decayTimer: ReturnType<typeof setTimeout>;
}

export class TypingIndicator {
  private _activeTypers = new Map<string, TypingState>();
  private _container?: HTMLElement;
  private _roomId?: UUID;

  setContainer(container: HTMLElement): void {
    this._container = container;
    container.style.cssText = 'height:20px;overflow:hidden;padding:0 12px;font-size:11px;color:var(--text-muted,#8899a6);line-height:20px;white-space:nowrap;text-overflow:ellipsis;';
  }

  setRoomId(roomId: UUID): void { this._roomId = roomId; }

  onTypingStart(data: TypingEventPayload): void {
    if (data.roomId !== this._roomId) return;
    const existing = this._activeTypers.get(data.userId);
    if (existing) {
      clearTimeout(existing.decayTimer);
      existing.decayTimer = this._decay(data.userId);
      return;
    }
    this._activeTypers.set(data.userId, {
      displayName: data.displayName,
      decayTimer: this._decay(data.userId),
    });
    this._render();
  }

  onTypingStop(data: TypingEventPayload): void {
    if (data.roomId !== this._roomId) return;
    const s = this._activeTypers.get(data.userId);
    if (s) { clearTimeout(s.decayTimer); this._activeTypers.delete(data.userId); this._render(); }
  }

  clearAll(): void {
    for (const s of this._activeTypers.values()) clearTimeout(s.decayTimer);
    this._activeTypers.clear();
    this._render();
  }

  get activeCount(): number { return this._activeTypers.size; }

  private _decay(userId: string) {
    return setTimeout(() => { this._activeTypers.delete(userId); this._render(); }, TYPING_DECAY_MS);
  }

  private _render(): void {
    if (!this._container) return;
    const names = Array.from(this._activeTypers.values()).map(t => t.displayName);
    if (names.length === 0) { this._container.textContent = ''; return; }
    this._container.textContent = `${names.join(', ')} ${names.length === 1 ? 'is' : 'are'} typing...`;
  }
}
