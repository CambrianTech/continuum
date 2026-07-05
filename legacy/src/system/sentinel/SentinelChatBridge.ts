/**
 * SentinelChatBridge — Posts sentinel lifecycle events to chat rooms.
 *
 * Subscribes to SentinelEventBridge events and posts progress updates
 * to the chat room associated with the sentinel. This makes sentinel
 * activity visible to the team — personas can see what's happening
 * and respond to completions/failures.
 *
 * Events posted:
 *   - Sentinel started (when watch is registered with a roomId)
 *   - Sentinel completed (with summary)
 *   - Sentinel failed (with error)
 *   - Sentinel cancelled
 *
 * Does NOT post step-by-step progress (too noisy). Personas can
 * query sentinel/status for details.
 */

import { Events } from '../core/shared/Events';
import { Commands } from '../core/shared/Commands';

/**
 * Event payload shape from SentinelEventBridge.
 */
interface SentinelBridgeEvent {
  handle: string;
  type: string;
  status?: string;
  personaId?: string;
  sentinelName?: string;
  roomId?: string;
  template?: string;
  exitCode?: number;
  durationMs?: number;
  error?: string;
}

/**
 * Tracks which handles we've already posted a start message for (avoid duplicates).
 */
const _announcedHandles = new Set<string>();

/**
 * Post a message to a chat room (fire-and-forget).
 */
async function postToChat(roomId: string, message: string, personaId?: string): Promise<void> {
  try {
    await Commands.execute('collaboration/chat/send', {
      room: roomId,
      message,
      // Use the persona's identity so the message appears from them, not "system"
      ...(personaId && { userId: personaId }),
    } as Record<string, unknown>);
  } catch (err) {
    console.warn(`[SentinelChatBridge] Failed to post to room ${roomId}: ${err}`);
  }
}

/**
 * Format duration for display.
 */
function formatDuration(ms: number | undefined): string {
  if (!ms) return 'unknown duration';
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60_000);
  const secs = Math.round((ms % 60_000) / 1000);
  return `${mins}m ${secs}s`;
}

/**
 * Handle sentinel completion event.
 */
function onSentinelComplete(event: SentinelBridgeEvent): void {
  const { handle, roomId, sentinelName, durationMs, personaId } = event;
  if (!roomId) return;

  const name = sentinelName || handle.slice(0, 8);
  const duration = formatDuration(durationMs);

  postToChat(roomId, [
    `**Sentinel completed:** ${name}`,
    `Duration: ${duration}`,
  ].join('\n'), personaId);

  _announcedHandles.delete(handle);
}

/**
 * Handle sentinel error event.
 */
function onSentinelError(event: SentinelBridgeEvent): void {
  const { handle, roomId, sentinelName, error, durationMs, status, personaId } = event;
  if (!roomId) return;

  const name = sentinelName || handle.slice(0, 8);
  const duration = formatDuration(durationMs);
  const label = status === 'cancelled' ? 'cancelled' : 'failed';

  postToChat(roomId, [
    `**Sentinel ${label}:** ${name}`,
    `Duration: ${duration}`,
    ...(error ? [`Error: ${error.slice(0, 200)}`] : []),
  ].join('\n'), personaId);

  _announcedHandles.delete(handle);
}

/**
 * Handle sentinel cancelled event.
 */
function onSentinelCancelled(event: SentinelBridgeEvent): void {
  onSentinelError({ ...event, status: 'cancelled' });
}

/**
 * Announce that a sentinel has started (called externally, not via event).
 */
export function announceSentinelStart(
  roomId: string,
  sentinelName: string,
  handle: string,
  personaName?: string,
  personaId?: string,
): void {
  if (_announcedHandles.has(handle)) return;
  _announcedHandles.add(handle);

  const actor = personaName ? `**${personaName}**` : 'A persona';
  postToChat(roomId, [
    `${actor} launched sentinel: **${sentinelName}**`,
    `Handle: \`${handle.slice(0, 12)}\` — query status with \`sentinel/status\``,
  ].join('\n'), personaId);
}

// ─── Initialization ────────────────────────────────────────────────────────────

let _initialized = false;

/**
 * Initialize the chat bridge by subscribing to sentinel events.
 * Safe to call multiple times (idempotent).
 */
export function initializeSentinelChatBridge(): void {
  if (_initialized) return;
  _initialized = true;

  Events.subscribe('sentinel:complete', (event: unknown) => {
    onSentinelComplete(event as SentinelBridgeEvent);
  });

  Events.subscribe('sentinel:error', (event: unknown) => {
    onSentinelError(event as SentinelBridgeEvent);
  });

  Events.subscribe('sentinel:cancelled', (event: unknown) => {
    onSentinelCancelled(event as SentinelBridgeEvent);
  });

  console.log('[SentinelChatBridge] Initialized — listening for sentinel lifecycle events');
}
