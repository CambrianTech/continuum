/**
 * PersonaMessageGate — Feeds the Rust-side message cache.
 *
 * Echo chamber detection is in Rust (Gate 6 of full_evaluate); this module
 * just subscribes to chat-message events and pushes each new message into
 * every registered persona's Rust cognition bridge.
 *
 * The post-inference adequacy gate that used to live here was the
 * Helper-only-path / TS-cognition-policy double anti-pattern Joel banned
 * in the 2026-05-16 architecture reset — deleted in #1309 (the call-site
 * in PersonaMessageEvaluator) + this file (the method itself). Per-persona
 * pre-inference should-respond (Rust #1284), admission (Rust #1121 PR-4),
 * and the resource-aware broker (#1299) are the gates now.
 */

import { Events } from '../../../core/shared/Events';
import { COLLECTIONS } from '../../../shared/Constants';
import type { ChatMessageEntity } from '../../../data/entities/ChatMessageEntity';
import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { RustCognitionBridge } from './RustCognitionBridge';
import { PersonaTimingConfig } from './PersonaTimingConfig';

export class PersonaMessageGate {
  // In-memory recent message cache — used for post-inference adequacy (needs ChatMessageEntity fields).
  // Echo chamber detection is now Rust-side (Gate 6 of full_evaluate).
  private static _recentMessages: Map<string, ChatMessageEntity[]> = new Map();
  private static _cacheInitialized = false;
  private static readonly MAX_CACHED_PER_ROOM = PersonaTimingConfig.messageCache.maxPerRoom;

  // Rust bridges to feed — all personas' bridges get message cache updates
  private static _rustBridges: Set<RustCognitionBridge> = new Set();

  private readonly personaId: UUID;
  private readonly personaName: string;
  private readonly log: (message: string, ...args: any[]) => void;

  constructor(
    personaId: UUID,
    personaName: string,
    log: (message: string, ...args: any[]) => void,
  ) {
    this.personaId = personaId;
    this.personaName = personaName;
    this.log = log;
    PersonaMessageGate.initMessageCache();
  }

  /**
   * Register a Rust bridge so it receives message cache updates.
   * Called once per persona after bridge initialization.
   */
  registerRustBridge(bridge: RustCognitionBridge): void {
    PersonaMessageGate._rustBridges.add(bridge);
  }

  /**
   * Unregister a Rust bridge during persona shutdown to prevent leaks.
   */
  static unregisterRustBridge(bridge: RustCognitionBridge | null): void {
    if (bridge) {
      PersonaMessageGate._rustBridges.delete(bridge);
    }
  }

  private static initMessageCache(): void {
    if (PersonaMessageGate._cacheInitialized) return;
    PersonaMessageGate._cacheInitialized = true;

    Events.subscribe(`data:${COLLECTIONS.CHAT_MESSAGES}:created`, (entity: any) => {
      const msg = entity as ChatMessageEntity;
      if (!msg.roomId) return;
      const roomId = msg.roomId;

      // TS-side cache (for post-inference adequacy)
      let messages = PersonaMessageGate._recentMessages.get(roomId);
      if (!messages) {
        messages = [];
        PersonaMessageGate._recentMessages.set(roomId, messages);
      }
      messages.push(msg);
      if (messages.length > PersonaMessageGate.MAX_CACHED_PER_ROOM) {
        messages.shift();
      }

      // Feed Rust-side cache (for echo chamber — Gate 6 of full_evaluate)
      const timestamp = msg.timestamp instanceof Date ? msg.timestamp.getTime() : new Date(msg.timestamp).getTime();
      for (const bridge of PersonaMessageGate._rustBridges) {
        bridge.cacheMessage(
          roomId,
          msg.id,
          msg.senderId,
          msg.senderType ?? 'human',
          msg.senderName ?? 'Unknown',
          msg.content?.text ?? '',
          timestamp,
        ).catch(() => { /* non-fatal */ });
      }
    });
  }

}
