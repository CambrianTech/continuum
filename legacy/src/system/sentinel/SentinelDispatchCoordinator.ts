/**
 * SentinelDispatchCoordinator — Prevents multiple personas from dispatching
 * sentinels for the same message.
 *
 * When a human sends "implement feature X", multiple personas may evaluate the
 * message and decide to dispatch a sentinel. This coordinator ensures only ONE
 * persona claims the dispatch — first come, first served.
 *
 * The claim is tied to a message ID and expires after a short TTL (prevents
 * stale claims from blocking future dispatches).
 *
 * Usage:
 *   if (SentinelDispatchCoordinator.claim(messageId, personaId)) {
 *     // This persona won — dispatch the sentinel
 *   } else {
 *     // Another persona already claimed this message — skip dispatch
 *   }
 */

import type { UUID } from '../core/types/CrossPlatformUUID';

interface DispatchClaim {
  messageId: UUID;
  personaId: UUID;
  template: string;
  timestamp: number;
}

/** How long a claim lives before expiring (5 minutes) */
const CLAIM_TTL_MS = 5 * 60 * 1000;

/** Maximum claims to track (prevents memory leaks) */
const MAX_CLAIMS = 100;

class SentinelDispatchCoordinatorImpl {
  private _claims = new Map<UUID, DispatchClaim>();

  /**
   * Attempt to claim sentinel dispatch for a message.
   * Returns true if this persona wins the claim, false if another persona already claimed it.
   */
  claim(messageId: UUID, personaId: UUID, template: string): boolean {
    this._evictStale();

    const existing = this._claims.get(messageId);
    if (existing) {
      // Already claimed — only the original claimant can proceed
      return existing.personaId === personaId;
    }

    // First claim wins
    this._claims.set(messageId, {
      messageId,
      personaId,
      template,
      timestamp: Date.now(),
    });

    return true;
  }

  /**
   * Check if a message has been claimed for sentinel dispatch.
   */
  isClaimed(messageId: UUID): boolean {
    this._evictStale();
    return this._claims.has(messageId);
  }

  /**
   * Get the persona that claimed a message.
   */
  claimant(messageId: UUID): UUID | undefined {
    this._evictStale();
    return this._claims.get(messageId)?.personaId;
  }

  /**
   * Release a claim (e.g., if sentinel launch fails and we want to allow retry).
   */
  release(messageId: UUID): void {
    this._claims.delete(messageId);
  }

  /**
   * Remove expired claims.
   */
  private _evictStale(): void {
    const now = Date.now();
    for (const [id, claim] of this._claims) {
      if (now - claim.timestamp > CLAIM_TTL_MS) {
        this._claims.delete(id);
      }
    }
    // Evict oldest if over limit
    if (this._claims.size > MAX_CLAIMS) {
      const sorted = Array.from(this._claims.entries())
        .sort((a, b) => a[1].timestamp - b[1].timestamp);
      const toRemove = sorted.slice(0, this._claims.size - MAX_CLAIMS);
      for (const [id] of toRemove) {
        this._claims.delete(id);
      }
    }
  }
}

/** Singleton coordinator — shared across all personas in the server process. */
export const SentinelDispatchCoordinator = new SentinelDispatchCoordinatorImpl();
