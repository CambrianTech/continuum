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
import type {
  ActReceiptView,
  ChatMessageView,
  RosterSlotView,
  SenderKind,
} from '@continuum/sdk-typescript';
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
  /** Citizens whose inbound stream admitted this line as a turn — "heard by N". */
  readonly heardBy?: number;
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

/** One tool act inside a receipt group — the expanded row (#243). */
export interface ActReceiptVM {
  readonly id: string;
  /** Opaque tool name as executed ("code/read"). */
  readonly tool: string;
  /** One-line human object ("sympy/core/mul.py"). Empty = verb-only. */
  readonly summary: string;
  readonly ok: boolean;
  readonly time: string;
}

/** A COLLAPSED run of consecutive tool acts by one actor — the transcript's
 *  receipt row (the Claude-iOS pattern: "Ran 3 commands ›" between speech;
 *  web expands in place, mobile opens a sheet). Grouping lives HERE, in the
 *  one projection, so web/tui/mobile all collapse identically. */
export interface ActGroupVM {
  readonly row: 'acts';
  /** First receipt's id — the group's stable render key. */
  readonly id: string;
  readonly actorId: string;
  readonly actorName: string;
  /** The iOS-style collapsed line: "Read 2 files, ran a command". */
  readonly summaryLine: string;
  /** Wall-clock of the group's LAST act (the freshest fact). */
  readonly time: string;
  /** True when ANY receipt in the group failed — the collapsed line warns. */
  readonly anyFailed: boolean;
  readonly receipts: readonly ActReceiptVM[];
}

/** One transcript row: a spoken message or a collapsed act group, discriminated
 *  on `row` (`kind` is taken — it's the author kind on message rows). */
export type TranscriptRowVM = ({ readonly row: 'message' } & MessageRowVM) | ActGroupVM;

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
  /** The FULL transcript: messages and collapsed act-receipt groups interleaved
   *  by timestamp (#243 — the room is the activity's full event stream; speech-
   *  only transcripts hide the work). Renderers draw THIS; `messages` remains
   *  the speech-only view other consumers (ticker, digests) keep reading. */
  readonly transcript: readonly TranscriptRowVM[];
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
    heardBy: msg.heard_by?.length ?? 0,
    ...(digest ? { digest } : {}),
  };
}

/** Consecutive-sender window: a message within this span of the previous row
 *  by the SAME sender renders as a continuation (no avatar/head) — the classic
 *  grouping that turns bubble-per-line sprawl into readable runs. */
const GROUP_WINDOW_MS = 5 * 60 * 1000;

/** Classify a tool name into the collapsed line's verb bucket. Read verbs are
 *  the same deny-list vocabulary the workspace invalidator speaks; everything
 *  unknown reads as generic "used a tool" — honest, never a guessed verb. */
function actVerb(tool: string): 'read' | 'searched' | 'edited' | 'ran' | 'used' {
  if (/^code\/(read|list|tree|glob)$/.test(tool)) return 'read';
  if (/^code\/search$/.test(tool)) return 'searched';
  if (/^code\/(write|edit)$/.test(tool)) return 'edited';
  if (/^(code\/(shell|run)|cargo\/|git\/)/.test(tool)) return 'ran';
  return 'used';
}

/** The iOS-style collapsed line: verb buckets with counts, in first-occurrence
 *  order — "Read 2 files, ran a command", "Edited a file, ran 3 commands". */
export function actSummaryLine(tools: readonly string[]): string {
  const nouns: Record<ReturnType<typeof actVerb>, [string, string]> = {
    read: ['a file', 'files'],
    searched: ['once', 'times'],
    edited: ['a file', 'files'],
    ran: ['a command', 'commands'],
    used: ['a tool', 'tools'],
  };
  const order: ReturnType<typeof actVerb>[] = [];
  const counts = new Map<ReturnType<typeof actVerb>, number>();
  for (const t of tools) {
    const v = actVerb(t);
    if (!counts.has(v)) order.push(v);
    counts.set(v, (counts.get(v) ?? 0) + 1);
  }
  const parts = order.map((v) => {
    const n = counts.get(v) ?? 0;
    const [one, many] = nouns[v];
    return n === 1 ? `${v} ${one}` : `${v} ${n} ${many}`;
  });
  const line = parts.join(', ');
  return line.charAt(0).toUpperCase() + line.slice(1);
}

/** Interleave messages and act receipts by timestamp into the transcript,
 *  collapsing consecutive same-actor acts (nothing between them) into one
 *  group row. Pure over the two wire arrays — the one place every client's
 *  collapse behavior comes from. */
function buildTranscript(
  messages: readonly MessageRowVM[],
  wireMessages: readonly ChatMessageView[],
  acts: readonly ActReceiptView[],
): TranscriptRowVM[] {
  type Ev =
    | { ts: number; msg: MessageRowVM }
    | { ts: number; act: ActReceiptView };
  const events: Ev[] = [
    ...messages.map((m, i) => ({ ts: wireMessages[i]?.timestamp ?? 0, msg: m })),
    ...acts.map((a) => ({ ts: a.timestamp, act: a })),
  ].sort((x, y) => x.ts - y.ts);

  const out: TranscriptRowVM[] = [];
  for (const ev of events) {
    if ('msg' in ev) {
      out.push({ row: 'message', ...ev.msg });
      continue;
    }
    const a = ev.act;
    const receipt: ActReceiptVM = {
      id: a.id,
      tool: a.tool,
      summary: a.summary,
      ok: a.ok,
      time: formatTimeOfDay(a.timestamp),
    };
    const last = out[out.length - 1];
    if (last && last.row === 'acts' && last.actorId === a.actor_id) {
      // Extend the open group: rebuild the immutable row with the new receipt.
      const receipts = [...last.receipts, receipt];
      out[out.length - 1] = {
        ...last,
        receipts,
        summaryLine: actSummaryLine(receipts.map((r) => r.tool)),
        time: receipt.time,
        anyFailed: last.anyFailed || !a.ok,
      };
    } else {
      out.push({
        row: 'acts',
        id: a.id,
        actorId: a.actor_id,
        actorName: a.actor_name,
        summaryLine: actSummaryLine([a.tool]),
        time: receipt.time,
        anyFailed: !a.ok,
        receipts: [receipt],
      });
    }
  }
  return out;
}

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
    // Additive wire field (#243): an older core omits `acts` → the transcript
    // is just the messages, exactly the pre-receipt surface.
    // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition
    transcript: buildTranscript(messages, state.messages, state.acts ?? []),
    isEmpty: state.messages.length === 0,
    revision: state.revision,
  };
}
