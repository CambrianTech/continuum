/**
 * `chatViewModel` — the pure projection from a `ChatState` snapshot to the flat,
 * render-ready view model the three-panel chat surface draws.
 *
 * This is where ALL the chat presentation logic lives, and it is deliberately
 * DOM-free, Lit-free and renderer-neutral: a plain `(ChatState) => ChatViewModel`
 * function, unit-tested without a browser. Every client renderer is then thin and
 * maps this view model to its own output — apps/web's Lit template (`renderChat`
 * → `<chat-widget>`) and apps/tui's ANSI renderer both consume the SAME model.
 * Keeping the logic in one pure function is the compression rule (one place
 * computes "how a message row reads") and the reason neither renderer needs a
 * jsdom or a live terminal to test its behavior.
 *
 * ## The three panels — who / what / where
 *
 * One `ChatViewState` snapshot carries all three facets of Joel's three-panel
 * design, so they project from one input:
 *   - **where/which** — `roomName` / `roomId` / member counts (the header).
 *   - **who** — `members` (the roster rail: who is present, live).
 *   - **what** — `messages` (the centre: the conversation itself).
 */

import type { ChatState } from './ChatState';
import type { ChatMessageView, RosterSlotView, SenderKind } from '@continuum/sdk-typescript';

/** The neutral author/member kind discriminant (`'human' | 'agent' | 'system'`). */
export type MemberKind = SenderKind['kind'];

/** One roster-rail entry — "who is here", rendered live off airc presence. */
export interface RosterMemberVM {
  readonly id: string;
  readonly name: string;
  readonly kind: MemberKind;
  /** Attached and ready to receive turns (drives the presence dot). */
  readonly active: boolean;
  /** Self-reported runtime origin (`"claude"`, `"codex"`, `""` = unresolved). */
  readonly runtime: string;
}

/** One conversation row — "what was said". */
export interface MessageRowVM {
  readonly id: string;
  readonly senderId: string;
  readonly senderName: string;
  readonly kind: MemberKind;
  readonly content: string;
  /** Wall-clock time-of-day (UTC `HH:MM`) — deterministic across machines. */
  readonly time: string;
  readonly runtime: string;
}

/** The full render-ready projection of a chat snapshot. */
export interface ChatViewModel {
  readonly roomName: string;
  readonly roomId: string;
  /** The room's activity purpose (the Content dispatch key — "chat", "foundry"…).
   *  Today always "chat"; when RoomPurposeSource (#6) lands, a client's `Content`
   *  registry dispatches on it (ACTIVITY-ROOM-PATTERNS.md). */
  readonly purpose: string;
  readonly memberCount: number;
  readonly activeCount: number;
  readonly members: readonly RosterMemberVM[];
  readonly messages: readonly MessageRowVM[];
  /** No messages yet — the surface renders an honest empty state, not an error. */
  readonly isEmpty: boolean;
  readonly revision?: number;
}

/** UTC `HH:MM` from unix-ms. Deterministic (no locale/timezone) so the view
 *  model is testable; a localizing formatter is a later presentation choice. */
export function formatTimeOfDay(unixMs: number): string {
  const d = new Date(unixMs);
  const hh = String(d.getUTCHours()).padStart(2, '0');
  const mm = String(d.getUTCMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

function memberVM(slot: RosterSlotView): RosterMemberVM {
  return {
    id: slot.member_id,
    name: slot.display_name,
    kind: slot.kind.kind,
    active: slot.active,
    runtime: slot.provenance.runtime,
  };
}

function messageVM(msg: ChatMessageView): MessageRowVM {
  return {
    id: msg.id,
    senderId: msg.sender_id,
    senderName: msg.sender_name,
    kind: msg.sender_kind.kind,
    content: msg.content,
    time: formatTimeOfDay(msg.timestamp),
    runtime: msg.provenance.runtime,
  };
}

/** Project a `ChatState` snapshot into the flat view model the panels render. */
export function chatViewModel(state: ChatState): ChatViewModel {
  const members = state.roster.map(memberVM);
  return {
    roomName: state.room_name,
    roomId: state.room_id,
    purpose: state.purpose,
    memberCount: members.length,
    activeCount: members.filter((m) => m.active).length,
    members,
    messages: state.messages.map(messageVM),
    isEmpty: state.messages.length === 0,
    revision: state.revision,
  };
}
