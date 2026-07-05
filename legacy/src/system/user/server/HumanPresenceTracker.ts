/**
 * HumanPresenceTracker - In-memory singleton tracking which room each human user is viewing.
 *
 * Personas use this for attention awareness: "Joel is viewing general right now."
 * Not surveillance — just basic presence, like seeing someone in a room.
 * No persistence, no history, no analytics. Just current state.
 *
 * Multi-user ready: tracks presence per userId so multiple humans work correctly.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { Events } from '../../core/shared/Events';
import { PRESENCE_EVENTS, type RoomActivePayload } from '../../core/shared/EventConstants';

interface HumanPresence {
  userId: string;
  displayName: string;
  roomId: UUID;
  roomName: string;
  since: number;
}

class HumanPresenceTrackerImpl {
  private _presenceByUser = new Map<string, HumanPresence>();
  private _initialized = false;

  /** Start listening for presence events. Call once at server startup. */
  initialize(): void {
    if (this._initialized) return;
    this._initialized = true;

    Events.subscribe(PRESENCE_EVENTS.ROOM_ACTIVE, (data: RoomActivePayload) => {
      this._presenceByUser.set(data.userId, {
        userId: data.userId,
        displayName: data.displayName,
        roomId: data.roomId as UUID,
        roomName: data.roomName,
        since: Date.now(),
      });
    });
  }

  /** The room a specific user is currently viewing (null if unknown/offline). */
  activeRoomFor(userId: string): { roomId: UUID; roomName: string } | null {
    const p = this._presenceByUser.get(userId);
    if (!p) return null;
    return { roomId: p.roomId, roomName: p.roomName };
  }

  /** Check if any human is currently viewing a specific room. */
  isViewingRoom(roomId: UUID): boolean {
    for (const p of this._presenceByUser.values()) {
      if (p.roomId === roomId) return true;
    }
    return false;
  }

  /** Get all users currently viewing a specific room. */
  viewersOf(roomId: UUID): HumanPresence[] {
    const viewers: HumanPresence[] = [];
    for (const p of this._presenceByUser.values()) {
      if (p.roomId === roomId) viewers.push(p);
    }
    return viewers;
  }

  /**
   * Full presence info for RAG context injection.
   * Returns first human's presence for backward compat (single-user systems).
   * Use allPresence for multi-user.
   */
  get presence(): HumanPresence | null {
    const first = this._presenceByUser.values().next();
    return first.done ? null : first.value;
  }

  /** All tracked human presences (multi-user). */
  get allPresence(): HumanPresence[] {
    return Array.from(this._presenceByUser.values());
  }
}

export const HumanPresenceTracker = new HumanPresenceTrackerImpl();
