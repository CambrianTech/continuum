/**
 * `<chat-widget>`'s presentation overlays on the projected view model: each takes
 * the model and the widget-owned state it overlays, and returns a new model. Moved
 * out of `ChatWidget.render` unchanged in behaviour; pure, so each is testable
 * without a browser.
 */
import type { ChatViewModel, MessageRowVM, RosterMemberVM } from '@continuum/chat-view';

/** `?demo`: sample cognition/genome/engine vitals to PREVIEW the rich persona
 *  readout before the cognition-emission slice wires the real signals. Explicit
 *  flag, never the live default (no fabricated data on a real room). */
export function withDemoVitals(vm: ChatViewModel): ChatViewModel {
  return {
    ...vm,
    members: vm.members.map((m, i) => {
      // A small spread of real model shapes so the LOADOUT strip previews the
      // formatting (B/T sizes, k/M ctx) — demo-only, never a live default.
      const models = [
        { model: 'devstral-24b', params: 24_000_000_000, contextWindow: 32_768 },
        { model: 'qwen3-coder-30b', params: 30_500_000_000, contextWindow: 262_144 },
        { model: 'claude-opus-4-8', params: 671_000_000_000, contextWindow: 1_000_000 },
      ];
      return {
        ...m,
        vitals: {
          focus: 30 + ((i * 27) % 60),
          reason: 80 - ((i * 19) % 55),
          recall: 45 + ((i * 23) % 45),
          act: 20 + ((i * 31) % 70),
          genome: 33 + ((i * 17) % 60),
          speed: 55 + ((i * 13) % 40),
          size: 40 + ((i * 21) % 45),
        },
        loadout: models[i % models.length],
      };
    }),
  };
}

/** Directory union: this room's live roster as-is; every other known member
 *  appended greyed. A citizens app showing zero citizens is broken by definition
 *  ([Joel, live]) — the panel is never blank again. Folds the room's members and
 *  the seed into `directory` (the widget's session-long memory of who exists). */
export function withDirectory(
  vm: ChatViewModel,
  directory: Map<string, RosterMemberVM>,
  seed: readonly RosterMemberVM[],
): ChatViewModel {
  for (const m of vm.members) directory.set(m.id, m);
  for (const m of seed) if (!directory.has(m.id)) directory.set(m.id, m);
  // Residency wins for liveness: the room's presence flag derives from
  // lane-readiness (away=warming), which greys a citizen precisely while
  // she's hardest at work. A resident on THIS node is online, full stop.
  const resident = new Set(seed.filter((m) => m.active).map((m) => m.id));
  // The directory's heartbeat recency beats a room card's stale stamp —
  // the "9d ago" rows were identity-creation dates, not activity.
  const seedSeen = new Map(seed.map((m) => [m.id, m.lastSeenMs]));
  // Faces ride the directory too: the presence stream's avatar enrichment
  // only lands on changed-roster publishes (heartbeats are channel-scoped,
  // so fossil rosters never re-publish) — the seed's portrait fills any
  // member the stream left faceless.
  const seedFace = new Map(seed.flatMap((m) => (m.avatarUrl ? [[m.id, m.avatarUrl] as const] : [])));
  const members = vm.members.map((m) => {
    const fresher = Math.max(m.lastSeenMs, seedSeen.get(m.id) ?? 0);
    const active = m.active || resident.has(m.id);
    const face = m.avatarUrl ?? seedFace.get(m.id);
    return active !== m.active || fresher !== m.lastSeenMs || face !== m.avatarUrl
      ? { ...m, active, lastSeenMs: fresher, ...(face !== undefined ? { avatarUrl: face } : {}) }
      : m;
  });
  const present = new Set(members.map((m) => m.id));
  // Liveness is NODE-level, not room-level: a resident citizen (and the
  // viewer) is online in EVERY room's directory — the seed re-polls that
  // truth. Only non-seed ghosts (remembered from other rooms' rosters)
  // grey out when absent here.
  const offRoom = [...directory.values()]
    .filter((m) => !present.has(m.id))
    .map((m) => ({ ...m, active: resident.has(m.id) }));
  // Working minds on top, the greyed past below — sorted by liveness then
  // recency then name ("the ONLINE users like benchy and atlas are greyed,
  // at the bottom and doing shit" — Joel, 2026-08-30).
  // STABLE order: online first, then NAME. lastSeenMs is deliberately NOT
  // a sort key — it refreshes on every directory poll, and a volatile sort
  // key made the whole rail (and the call grid downstream) visibly
  // reshuffle on every render tick (Joel, live 2026-08-31: "all the boxes
  // randomly reordered"). Recency still renders as the row's stamp; it
  // just doesn't move rows around.
  const merged = [...members, ...offRoom].sort(
    (a, b) => Number(b.active) - Number(a.active) || a.name.localeCompare(b.name),
  );
  return {
    ...vm,
    members: merged,
    memberCount: merged.length,
    activeCount: merged.filter((m) => m.active).length,
  };
}

/** Digest tier ([[perception-resolution-contract]]): stamp the reader's expand
 *  choices onto the rows the projection classified — an expanded row renders its
 *  full body with a collapse affordance; everything else stays digested. */
export function withExpanded(vm: ChatViewModel, expanded: ReadonlySet<string>): ChatViewModel {
  if (expanded.size === 0) return vm;
  return {
    ...vm,
    messages: vm.messages.map((m) => (expanded.has(m.id) ? { ...m, expanded: true } : m)),
  };
}

/** #170 live typing: overlay a transient bubble per persona mid-turn, growing
 *  token-by-token, so she visibly types instead of freezing. Resolve each sender's
 *  name/kind from the roster (or a prior message) — skip if unknown, to never
 *  fabricate an identity. */
export function withTyping(vm: ChatViewModel, typing: ReadonlyMap<string, string>): ChatViewModel {
  if (typing.size === 0) return vm;
  const typingRows: MessageRowVM[] = [];
  for (const [senderId, text] of typing) {
    // (No last-sender suppression: a persona speaking again right after
    // their own message is NORMAL — with echo/settle-lag making consecutive
    // turns common, the old skip hid live streams exactly when Joel was
    // watching for them. The settle-matching retire handles dups.)
    const member = vm.members.find((m) => m.id === senderId);
    const prior = vm.messages.find((m) => m.senderId === senderId);
    const senderName = member?.name ?? prior?.senderName;
    const kind = member?.kind ?? prior?.kind;
    if (senderName === undefined || kind === undefined) continue;
    // A stream with NO text yet (the #254 start beacon: turn dispatched,
    // prefill running) gets NO bubble — it reports through the grey
    // "X is responding…" line above the compose box instead. Only streams
    // with actual tokens render as live bubbles.
    if (text.length === 0) continue;
    typingRows.push({
      id: `typing:${senderId}`,
      senderId,
      senderName,
      kind,
      content: `${text}▋`,
      time: '',
      runtime: member?.runtime ?? prior?.runtime ?? '',
    });
  }
  const withBubbles =
    typingRows.length === 0
      ? vm
      : {
          ...vm,
          messages: [...vm.messages, ...typingRows],
          transcript: [...vm.transcript, ...typingRows.map((r) => ({ row: 'message' as const, ...r }))],
          isEmpty: false,
        };
  // The tile's SPEAKING ring: overlay the live token rail onto the roster
  // vitals (`speaking: 100`) for members mid-turn — the same widget-owned
  // overlay pattern as the typing bubbles above and MessageRowVM.expanded.
  // Driven only by REAL StreamDeltas; retires when the turn's `done` lands.
  return {
    ...withBubbles,
    members: withBubbles.members.map((m) =>
      typing.has(m.id) ? { ...m, vitals: { ...m.vitals, speaking: 100 } } : m,
    ),
  };
}

/** "X is responding…" — the names for the grey status line BETWEEN the last
 *  message and the compose box (Joel's spec, the Discord/Slack typing-line
 *  convention). Driven by the stream map: the #254 start beacon adds a persona
 *  here the moment their turn dispatches, minutes before the first token on a
 *  cold lane — the third dead-looking-room scare of 2026-07-30 was four busy minds
 *  with zero pixels; this line is the cure. */
export function respondingNames(vm: ChatViewModel, typing: ReadonlyMap<string, string>): string[] {
  const responders: string[] = [];
  for (const [senderId, text] of typing) {
    // The line covers the PROMISE phase only — inference dispatched, no
    // visible words yet. Once tokens stream into their bubble, the persona
    // is visibly responding and their name drops from the line (Joel: "it's
    // when we are sure they're gonna respond — then you can show it").
    if (text.length > 0) continue;
    const name =
      vm.members.find((m) => m.id === senderId)?.name ??
      vm.messages.find((m) => m.senderId === senderId)?.senderName;
    if (name !== undefined) responders.push(name);
  }
  return responders;
}
