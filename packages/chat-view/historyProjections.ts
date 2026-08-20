/**
 * History projection — durable `chat/poll` pages → transcript rows.
 *
 * The Twitter endless-scroll's render half. `chat/poll { beforeMessageId }`
 * returns raw `ChatMessageEntity` payloads straight from durable storage
 * (`{ id, roomId, senderId, timestamp: ISO, content: { text }, metadata }`) —
 * a DIFFERENT wire shape from the live projection's `ChatMessageView` (which
 * carries resolved sender names). This maps one storage page onto the same
 * `MessageRowVM` rows the live tail renders, resolving identity through the
 * roster the view model already holds — one row shape on screen, whether the
 * message arrived live or was paged out of history.
 *
 * Trigger idioms stay per-target (IntersectionObserver / scroll threshold on
 * web, ScrollController on Flutter, PgUp in a TUI); THIS mapping is the shared
 * mechanics ([[one-logical-decision-one-place]]).
 */

import { formatTimeOfDay } from './chatViewModel';
import type { MemberKind, MessageRowVM, RosterMemberVM } from './chatViewModel';

/** The storage-entity fields this projection reads. Anything missing renders
 *  honestly degraded (short-id sender, empty time) — never a thrown page. */
interface StoredMessageLike {
  readonly id?: unknown;
  readonly senderId?: unknown;
  readonly timestamp?: unknown;
  readonly content?: unknown;
  readonly metadata?: unknown;
}

/** Extract the text body from the stored `content` — `{ text }` today; a bare
 *  string is tolerated for forward-compat with externalized media captions. */
function textOf(content: unknown): string {
  if (typeof content === 'string') return content;
  if (typeof content === 'object' && content !== null) {
    const text = (content as { text?: unknown }).text;
    if (typeof text === 'string') return text;
  }
  return '';
}

/** Resolve a stored sender to display identity: the roster first (same names
 *  the live tail shows), else `metadata.source` discriminates human/agent and
 *  the short-id stands in for the name — honest, never fabricated. */
function senderOf(
  senderId: string,
  metadata: unknown,
  members: readonly RosterMemberVM[],
): { name: string; kind: MemberKind } {
  const member = members.find((m) => m.id === senderId);
  if (member) return { name: member.name, kind: member.kind };
  const source =
    typeof metadata === 'object' && metadata !== null
      ? (metadata as { source?: unknown }).source
      : undefined;
  return {
    name: senderId.slice(0, 8),
    kind: source === 'user' ? 'human' : 'agent',
  };
}

/** One `chat/poll` page (raw stored entities, chronological) → transcript rows.
 *  Skips records with no usable id (a malformed row must not poison the page)
 *  and dedups against `excludeIds` (the live tail — an anchor-timestamp tie can
 *  return a row the screen already shows; showing it twice reads as a bug). */
export function historyRowsFromPoll(
  entities: readonly unknown[],
  members: readonly RosterMemberVM[],
  excludeIds?: ReadonlySet<string>,
): MessageRowVM[] {
  const rows: MessageRowVM[] = [];
  for (const raw of entities) {
    if (typeof raw !== 'object' || raw === null) continue;
    const e = raw as StoredMessageLike;
    const id = typeof e.id === 'string' ? e.id : undefined;
    if (id === undefined || (excludeIds !== undefined && excludeIds.has(id))) continue;
    const senderId = typeof e.senderId === 'string' ? e.senderId : '';
    const { name, kind } = senderOf(senderId, e.metadata, members);
    // Stored timestamps are ISO strings; the live wire carries unix ms. Both
    // land on the same deterministic UTC HH:MM the transcript shows.
    const ms = typeof e.timestamp === 'string' ? Date.parse(e.timestamp) : Number.NaN;
    rows.push({
      id,
      senderId,
      senderName: name,
      kind,
      content: textOf(e.content),
      time: Number.isNaN(ms) ? '' : formatTimeOfDay(ms),
      runtime: '',
    });
  }
  return rows;
}
