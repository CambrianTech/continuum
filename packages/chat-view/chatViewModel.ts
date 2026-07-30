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
import { messageDigest, type MessageDigestVM } from './messageDigest';

/** The neutral author/member kind discriminant (`'human' | 'agent' | 'system'`). */
export type MemberKind = SenderKind['kind'];

/** A member's **loadout** — the model backing it (`model · size · ctx`), the
 *  glass-box tile's LOADOUT strip. Every field optional: an honest absent when
 *  the substrate hasn't resolved it, never a fabricated capability. `params` is
 *  the RAW parameter count (the renderer formats `24_000_000_000` → "24B");
 *  `contextWindow` the raw token window (`32768` → "32k"). */
export interface LoadoutVM {
  readonly model?: string;
  readonly params?: number;
  readonly contextWindow?: number;
}

/** One roster-rail entry — "who is here", rendered live off airc presence. */
export interface RosterMemberVM {
  readonly id: string;
  readonly name: string;
  readonly kind: MemberKind;
  /** Attached and ready to receive turns (drives the presence dot). */
  readonly active: boolean;
  /** Self-reported runtime origin (`"claude"`, `"codex"`, `""` = unresolved). */
  readonly runtime: string;
  /** Opaque live **vitals** — normalized `0..=100` readouts (energy, attention,
   *  compute, …) the source attaches for the roster to draw as meters. Empty =
   *  none reported (a human, a remote peer, or a persona not surfacing state) —
   *  the card simply draws no meters, never fabricated bars. */
  readonly vitals: Record<string, number>;
  /** The model backing this member (`model · size · ctx`). Absent for a human,
   *  a remote peer, or a persona whose binding hasn't resolved — the card draws
   *  no LOADOUT strip, never a fabricated model. */
  readonly loadout?: LoadoutVM;
  /** WHEN this member was last active — raw epoch ms (`last_seen_ms` from
   *  presence). `0` = unreported; the card draws no recency stamp then, never a
   *  fabricated one. The renderer formats the idiom (`"55m ago"`). */
  readonly lastSeenMs: number;
  /** URL of this member's stored avatar image (`/avatars/<peer-id>.png`),
   *  when the producing node has one. Absent = the card draws its kind
   *  glyph — honest fallback, never a broken image. */
  readonly avatarUrl?: string;
  /** NAMES of the member's loaded skill overlays (paged-in LoRA genes), in
   *  load order — the label half of `vitals.genome` (which is only a
   *  normalized count), so the genome segments carry real adapter names.
   *  Absent = none loaded/reported — never fabricated labels. */
  readonly genes?: readonly string[];
  /** Pronouns from the member's published airc identity card (#262).
   *  Absent = no card published — the row shows nothing, never a guess. */
  readonly pronouns?: string;
  /** One-tag role from the identity card (free-form, verbatim). */
  readonly roleLabel?: string;
  /** One-sentence bio from the identity card — surfaced as the row's
   *  hover tooltip (and later the citizen page). Absent = no card. */
  readonly bio?: string;
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
  /** The digest tier for an over-threshold body ([[perception-resolution-contract]]):
   *  head + mechanical tail summary (+ repetition histogram). Absent = the full
   *  tier — render `content` verbatim. `content` always carries the untouched
   *  original either way; the digest never destroys fidelity, only defers it. */
  readonly digest?: MessageDigestVM;
  /** Renderer-owned overlay: the reader expanded this collapsed row (render the
   *  full body, offer "collapse"). NEVER set by the projection — the widget's
   *  expand state stamps it on, the same overlay pattern as live typing rows. */
  readonly expanded?: boolean;
  /** The sender's avatar image, joined from the roster at projection time so a
   *  message row draws the same face as the sender's tile. Absent = glyph. */
  readonly senderAvatarUrl?: string;
  /** True when this row continues the previous row's sender within the group
   *  window — the renderer draws a compact continuation (no avatar/head), the
   *  classic chat grouping that kills the bubble-per-line sprawl. */
  readonly continues?: boolean;
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

/** `HH:MM` from unix-ms in the VIEWER's timezone — a transcript timestamp is
 *  presentation for the person reading it (glass-boxed live 2026-07-30: hard
 *  UTC read "4:36" at 11:36 PM local — wrong for every human off-meridian).
 *  Tests stay deterministic by pinning `TZ=UTC` in the test scripts, not by
 *  hardcoding UTC into the product. */
export function formatTimeOfDay(unixMs: number): string {
  const d = new Date(unixMs);
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  return `${hh}:${mm}`;
}

/** Project the wire `Loadout` into the flat VM — dropping empty fields so an
 *  all-absent loadout collapses to `undefined` (the card draws no strip, never
 *  an empty one). `context_window` (snake, wire) → `contextWindow` (camel, VM). */
function loadoutVM(lo: RosterSlotView['loadout']): LoadoutVM | undefined {
  if (!lo) return undefined;
  const out: LoadoutVM = {
    ...(lo.model ? { model: lo.model } : {}),
    ...(lo.params ? { params: lo.params } : {}),
    ...(lo.context_window ? { contextWindow: lo.context_window } : {}),
  };
  return out.model || out.params !== undefined || out.contextWindow !== undefined
    ? out
    : undefined;
}

function memberVM(slot: RosterSlotView): RosterMemberVM {
  const loadout = loadoutVM(slot.loadout);
  return {
    id: slot.member_id,
    name: slot.display_name,
    kind: slot.kind.kind,
    active: slot.active,
    runtime: slot.provenance.runtime,
    // Additive field (#vitals): an older core omits it → treat as no vitals, the
    // same back-compat discipline as `purpose` ([[fallbacks-are-illegal-fail-loud]]).
    // The wire type marks vitals required (serde default), but a pre-field core sends
    // it undefined at runtime — the guard is real despite the type, so silence the
    // "unnecessary condition" the type-only view infers.
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
    vitals: slot.vitals ?? {},
    // Additive field (#186 loadout): absent for a human / unresolved agent — the
    // card draws no LOADOUT strip, never a fabricated model.
    ...(loadout ? { loadout } : {}),
    // Recency (card 2661a1b1): the raw presence signal, formatted by the renderer.
    // An older core omitting the field reads as 0 = unreported (no stamp drawn).
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
    lastSeenMs: slot.last_seen_ms ?? 0,
    // Additive field: the node's stored avatar image URL — absent = glyph fallback.
    ...(slot.avatar_url ? { avatarUrl: slot.avatar_url } : {}),
    // Additive field (QUE/genes revival): gene NAMES, only when the radiator
    // reported any — an older core omits the field entirely.
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
    ...(slot.genes && slot.genes.length > 0 ? { genes: slot.genes } : {}),
    // Additive fields (#262 identity cards): pronouns / role / bio from the
    // member's published airc card — absent when no card, never fabricated.
    ...(slot.pronouns ? { pronouns: slot.pronouns } : {}),
    ...(slot.role_label ? { roleLabel: slot.role_label } : {}),
    ...(slot.bio ? { bio: slot.bio } : {}),
  };
}

function messageVM(msg: ChatMessageView): MessageRowVM {
  // Digest tier ([[perception-resolution-contract]]): classify here, in the ONE
  // place that computes "how a message row reads", so every renderer (web, tui,
  // RAG grounding) inherits flood-proofing without re-deriving it.
  const digest = messageDigest(msg.content);
  return {
    id: msg.id,
    senderId: msg.sender_id,
    senderName: msg.sender_name,
    kind: msg.sender_kind.kind,
    content: msg.content,
    time: formatTimeOfDay(msg.timestamp),
    runtime: msg.provenance.runtime,
    ...(digest ? { digest } : {}),
  };
}

/** Consecutive-sender window: a message within this span of the previous row
 *  by the SAME sender renders as a continuation (no avatar/head) — the classic
 *  grouping that turns bubble-per-line sprawl into readable runs. */
const GROUP_WINDOW_MS = 5 * 60 * 1000;

/** Project a `ChatState` snapshot into the flat view model the panels render. */
export function chatViewModel(state: ChatState): ChatViewModel {
  const members = state.roster.map(memberVM);
  // Join each message to its sender's tile face + fold consecutive-sender
  // grouping — both here, in the ONE projection, so web/tui/RAG all inherit.
  const avatarBySender = new Map(
    state.roster.flatMap((m) => (m.avatar_url ? [[m.member_id, m.avatar_url] as const] : [])),
  );
  const messages = state.messages.map((msg, i): MessageRowVM => {
    const vm = messageVM(msg);
    const prev = state.messages[i - 1];
    const continues =
      prev !== undefined &&
      prev.sender_id === msg.sender_id &&
      msg.timestamp - prev.timestamp < GROUP_WINDOW_MS;
    const avatar = avatarBySender.get(msg.sender_id);
    return {
      ...vm,
      ...(avatar ? { senderAvatarUrl: avatar } : {}),
      ...(continues ? { continues: true } : {}),
    };
  });
  return {
    roomName: state.room_name,
    roomId: state.room_id,
    // `purpose` is an additive field (#1757). A ChatViewState is definitionally a
    // chat activity, and the server default is "chat", so an older/other core that
    // omits it means "chat" — a legitimate back-compat default, not a fallback that
    // hides a bug (a foundry room sends ForgeViewState, never a purpose-less chat).
    purpose: state.purpose || 'chat',
    memberCount: members.length,
    activeCount: members.filter((m) => m.active).length,
    members,
    messages,
    isEmpty: state.messages.length === 0,
    revision: state.revision,
  };
}
