/**
 * Web chat client entry — wires the SDK to the `<chat-widget>` view.
 *
 * This is the ONLY file that touches both the SDK and the DOM host; it is the
 * composition root Joel's three-panel design hangs off of. Two sockets to the
 * same core WS ingress, each doing one thing:
 *   - READ  — a `StateConnection` subscribed to `kind="chat"`. Every envelope is
 *     merged into a `ChatState` and pushed onto `widget.state`; Lit re-renders
 *     the who/what/where panels. This is the positron read surface (#84) the
 *     persona also observes — same substrate, different client.
 *   - SEND  — a `Continuum` command client. "Talk to Asha normally" = one
 *     `chat/send` into the room the widget is currently showing. Asha's reply
 *     (and the echo of our own turn) arrives back through the READ stream, so
 *     there is no optimistic local append to drift out of sync.
 *
 * The widget itself imports neither socket — the app owns the wiring, the widget
 * owns the view ([[headless-core-many-clients]], [[persona-is-a-client]]).
 */

import './theme.css';
import {
  Continuum,
  WebSocketTransport,
  StateConnection,
  IndexedDbStateStorage,
  buildCommandUri,
  type StateEnvelope,
} from '@continuum/sdk-typescript';
import { resolveConfig } from './config';
import {
  ChatWidget,
  type CloseTabHandler,
  type HistoryHandler,
  type SelectRoomHandler,
  type SendHandler,
  type SettingsHandler,
} from './chat/ChatWidget';
import {
  CHAT_KIND,
  KANBAN_KIND,
  NAV_KIND,
  SERVING_KIND,
  SYSTEM_METRICS_KIND,
  chatStateFromEnvelope,
  kanbanStateFromEnvelope,
  navStateFromEnvelope,
  servingFromEnvelope,
  BENCH_KIND,
  benchFromEnvelope,
  CANVAS_KIND,
  canvasFromEnvelope,
  systemMetricsFromEnvelope,
  type ChatState,
  type RosterMemberVM,
} from '@continuum/chat-view';

// Importing the module registers `<chat-widget>` as a side effect; keep the
// symbol referenced so bundlers don't tree-shake the definition away.
void ChatWidget;

async function main(): Promise<void> {
  const config = resolveConfig();

  // Citizen-scope the session: `?me=<uuid>` on the connect URL is WHO this
  // session belongs to — the core resolves per-user views (kind="nav") to this
  // citizen's substrate and spawns their nav projector on first arrival.
  const scopedWsUrl = `${config.wsUrl}${config.wsUrl.includes('?') ? '&' : '?'}me=${config.senderId}`;

  const widget = document.createElement('chat-widget');
  widget.callUrl = config.callUrl;
  // The version badge's real source: this build's package version, stamped by vite.
  widget.version = `v${__APP_VERSION__}`;
  // `?live` — boot straight into the focused room's LIVE face (the same state
  // the header's Go-live affordance toggles): a deep link to the call grid,
  // presentation state only ([[navigation-is-airc-state-one-semantics-many-idioms]]
  // — the URL is the web idiom; recipe-declared live rooms are the substrate path).
  if (new URLSearchParams(location.search).has('live')) widget.liveFace = true;
  const mount = document.getElementById('app') ?? document.body;
  mount.replaceChildren(widget);

  // The latest snapshot the READ stream has delivered — the SEND path reads its
  // `room_id` so a message always targets the room on screen.
  let latest: ChatState | undefined;

  // SEND socket: the command client. Fails loud if the send lands before any
  // snapshot named a room (no room to send into is a real error, not a no-op).
  const transport = new WebSocketTransport(scopedWsUrl);
  const continuum = Continuum.connect(transport);
  const sendHandler: SendHandler = async (text: string) => {
    if (!latest) {
      throw new Error('cannot send before the first room snapshot arrived — the room is unknown.');
    }
    const result = await continuum.commands.execute('chat/send', {
      roomId: latest.room_id,
      senderId: config.senderId,
      text,
    });
    // A kernel-level failure already rejected in the transport (this line never
    // runs). Belt-and-suspenders for any handler that instead reports failure
    // in-band: ONLY an explicit `success === false` throws. A success payload
    // may carry NO success field at all ({eventId, messageId} — glass-boxed
    // live 2026-07-30: `!result.success` showed "Send failed" on every send
    // that actually LANDED, the exact bug shape the history handler had).
    if ((result as { success?: boolean }).success === false) {
      throw new Error(`chat/send rejected: ${result.error ?? 'unknown error'}`);
    }
    // A `warning` on a success means stored-locally-but-broadcast-failed. Surface
    // it loud; the message did persist, so this is not a failure to throw on.
    if (result.warning) {
      console.warn(`chat/send partial: ${result.warning}`);
    }
  };
  widget.sendHandler = sendHandler;

  // Room switching: a rooms-rail pick is one `nav/select` into the core — the
  // NavIntent verb, not a client-side tab swap. The core writes the citizen's
  // focus, marks the left room read, and refocuses the chat projection; the
  // active cell + center pane move when those envelopes stream back through the
  // READ socket. No optimistic local state — substrate truth only, same
  // discipline as chat send.
  //
  // Dispatch rides the SAME facade seam `commands.execute` wraps
  // (buildCommandUri + transport.execute): `nav/select` is a registered typed
  // command core-side (modules/nav.rs), but the generated CommandMap predates
  // it and its re-emit is blocked by pre-existing drift (registered commands
  // with unexported wire types — see the sdk_codegen ts-codegen emit test).
  // When that regenerates, this becomes `continuum.commands.execute('nav/select', …)`.
  // Bare-wire contract: failure is a REJECTED promise (no success field), which
  // the widget surfaces — never a silently-dead click. `userId` is the command
  // envelope's caller-identity sibling (CommandRequest), same as nav/mark-read.
  // `kind` rides the verb (NavSelectParams.kind): 'room' switches the room on
  // screen; 'persona' opens that citizen's HOME tab (profile/brain) while the
  // chat projection stays pinned — the content dispatch keys off the tab kind.
  const selectRoomHandler: SelectRoomHandler = async (target: string, kind: 'room' | 'persona') => {
    await transport.execute(
      buildCommandUri('nav/select'),
      JSON.stringify({ userId: config.senderId, target, kind }),
    );
    // The address bar mirrors the focused activity as its URI short form —
    // `/room/<name>` · `/persona/<name>` — bookmarkable and shareable (Joel,
    // 2026-08-30: "url subpath matches uri"). The click path hands us the
    // UUID (identity); the nav state supplies the short name (display) when
    // it knows the tab. replaceState: navigation-in-place, not history spam.
    const known = widget.nav?.open_tabs?.find((t) => t.id === target);
    const short = known?.title ? known.title.toLowerCase() : target;
    window.history.replaceState(null, '', `/${kind}/${encodeURIComponent(short)}`);
  };
  widget.selectRoomHandler = selectRoomHandler;

  // A run card is a DOOR (bench-run-open, renderBench): clicking it stands
  // you in that run's activity room — same navigation verb as a tab click.
  // A worker's NAME on a bench card doors to her page; the wire carries the
  // display name, the directory seed resolves it to the durable id.
  widget.addEventListener('persona-open-by-name', (e: Event) => {
    const name = (e as CustomEvent<{ name?: string }>).detail?.name?.toLowerCase();
    if (name === undefined || name.length === 0 || name === 'unclaimed') return;
    const hit = widget.directorySeed.find((m) => m.name.toLowerCase() === name);
    if (hit) {
      void selectRoomHandler(hit.id, 'persona').catch((err: unknown) => {
        console.error('persona-open-by-name failed:', err);
      });
    }
  });

  widget.addEventListener('bench-run-open', (e: Event) => {
    const detail = (e as CustomEvent<{ roomId?: string; roomName?: string }>).detail;
    const roomId = detail?.roomId;
    if (typeof roomId !== 'string' || roomId.length === 0) return;
    void (async () => {
      try {
        // Standing in a room requires MEMBERSHIP: join first (by name — the
        // derived-channel law), then select. Without the name (a pre-naming
        // room) selection alone still works when already a member.
        if (detail.roomName !== undefined && detail.roomName.length > 0) {
          await transport.execute(
            buildCommandUri('room/join'),
            JSON.stringify({ userId: config.senderId, room: detail.roomName }),
          );
        }
        await selectRoomHandler(roomId, 'room');
      } catch (err) {
        console.error('bench-run-open navigation failed:', err);
      }
    })();
  });

  // Bookmark restore: a deep-linked URL (`/room/general`) re-selects that
  // activity — resolved against the nav state, because `nav/select` is
  // UUID-typed (names are display, ids are identity). The pending link waits
  // for the first nav envelope that knows the target, fires once, clears.
  let pendingDeepLink: { kind: 'room' | 'persona'; target: string } | null = null;
  let focusPinned = false;
  {
    const m = /^\/(room|persona)\/(.+)$/.exec(window.location.pathname);
    if (m?.[1] !== undefined && m[2] !== undefined) {
      pendingDeepLink = {
        kind: m[1] as 'room' | 'persona',
        target: decodeURIComponent(m[2]),
      };
    }
  }
  const UUID_LINK = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
  const resolveDeepLink = (tabs: readonly { id?: string; title?: string }[]): void => {
    if (!pendingDeepLink) return;
    // A UUID target IS the address — select it directly, no nav lookup
    // (solve rooms are navigable before they ever appear in a tab set).
    if (UUID_LINK.test(pendingDeepLink.target)) {
      const { kind, target } = pendingDeepLink;
      void selectRoomHandler(target, kind)
        .then(() => {
          pendingDeepLink = null; // consumed only on SUCCESS
        })
        .catch(() => {
          // Transport not up yet (page-load race) — the next nav envelope
          // retries; the link is only consumed when the select lands.
        });
      return;
    }
    const want = pendingDeepLink.target.toLowerCase();
    const hit = tabs.find(
      (t) => t.id?.toLowerCase() === want || t.title?.toLowerCase() === want,
    );
    if (hit?.id === undefined) return; // nav doesn't know it yet — next envelope
    const { kind } = pendingDeepLink;
    pendingDeepLink = null;
    void selectRoomHandler(hit.id, kind).catch((err: unknown) => {
      console.error(`deep link ${window.location.pathname} failed:`, err);
    });
  };

  // Settings: fetch or mutate the node's operator settings through the SAME
  // core verbs the terminal uses — `genome/sharing` (covenant consent + HF
  // identity; --agree records/revokes) and `genome/list` (the gene registry).
  // Same raw-wire seam + drift note as nav/select above. The face renders
  // whatever these verbs answer — substrate truth, one consent receipt across
  // every surface.
  const settingsHandler: SettingsHandler = async (agree?: boolean) => {
    const sharingRaw = await transport.execute(
      buildCommandUri('genome/sharing'),
      JSON.stringify(agree === undefined ? { userId: config.senderId } : { userId: config.senderId, agree }),
    );
    const sharing = JSON.parse(sharingRaw) as {
      agreed: boolean;
      covenant_version: string;
      receipt?: string;
      covenant: string;
      hf_account?: string;
    };
    const listRaw = await transport.execute(
      buildCommandUri('genome/list'),
      JSON.stringify({ userId: config.senderId }),
    );
    const list = JSON.parse(listRaw) as {
      genes: {
        gene: string;
        base_model: string;
        signed: boolean;
        trials: number;
        decayed_lift?: number;
      }[];
    };
    return {
      loaded: true,
      agreed: sharing.agreed,
      covenantVersion: sharing.covenant_version,
      ...(sharing.receipt !== undefined ? { receipt: sharing.receipt } : {}),
      covenant: sharing.covenant,
      ...(sharing.hf_account !== undefined ? { hfAccount: sharing.hf_account } : {}),
      genes: list.genes.map((g) => ({
        gene: g.gene,
        baseModel: g.base_model,
        signed: g.signed,
        trials: g.trials,
        ...(g.decayed_lift !== undefined ? { decayedLift: g.decayed_lift } : {}),
      })),
    };
  };
  widget.settingsHandler = settingsHandler;

  // Scroll-back: one older page out of the durable transcript per call —
  // `chat/poll { beforeMessageId }` is the storage cursor (the Twitter
  // endless-scroll's read half); the widget owns the trigger + the buffer.
  // Same raw-wire seam as `nav/select` above: the generated CommandMap
  // predates `beforeMessageId` and its re-emit is blocked by the same
  // unexported-wire-types drift — when that regenerates, this becomes
  // `continuum.commands.execute('chat/poll', …)`.
  let chatHistoryAbsenceLogged = false;
  const historyHandler: HistoryHandler = async (roomId, beforeMessageId) => {
    // THE DURABLE TRANSCRIPT (chat/history, 2026-08-31): the airc daemon's
    // own store — citizen speech AND radiated 💭/⚙ receipts, the full story
    // of an activity. chat/poll (operator collection) remains the fallback
    // for cores that predate the verb; rows are adapted to the one parser.
    try {
      const rawH = await transport.execute(
        buildCommandUri('chat/history'),
        JSON.stringify({ roomId, limit: 50 }),
      );
      const hist = JSON.parse(rawH) as {
        messages?: readonly { id: string; senderId: string; text: string; timestamp: number }[];
      };
      if (hist.messages !== undefined) {
        return hist.messages.map((m) => ({
          id: m.id,
          senderId: m.senderId,
          content: { text: m.text },
          timestamp: new Date(m.timestamp).toISOString(),
        }));
      }
    } catch (err) {
      // CLASSIFIED, never swallowed: only "this core predates the verb"
      // falls through to chat/poll; every other failure is a real defect
      // and propagates to the same error strip a failed poll reaches.
      const msg = err instanceof Error ? err.message : String(err);
      const verbAbsent = /no policy grants|unknown command|no legacy handler|not found/i.test(msg);
      if (!verbAbsent) throw err;
      if (!chatHistoryAbsenceLogged) {
        chatHistoryAbsenceLogged = true;
        console.info('chat/history not on this core — history degrades to chat/poll:', msg);
      }
    }
    const raw = await transport.execute(
      buildCommandUri('chat/poll'),
      JSON.stringify({ roomId, beforeMessageId, limit: 50 }),
    );
    // Bare-wire contract (same as nav/select above): a FAILURE is a rejected
    // promise; a success payload is the command output and need not carry a
    // `success` field — only an EXPLICIT false is an in-band rejection.
    // (`!result.success` here read every successful page as a failure —
    // glass-boxed live as "History load failed … unknown error".)
    const result = JSON.parse(raw) as {
      success?: boolean;
      error?: string;
      messages?: readonly unknown[];
    };
    if (result.success === false) {
      throw new Error(`chat/poll rejected: ${result.error ?? 'unknown error'}`);
    }
    return result.messages ?? [];
  };
  widget.historyHandler = historyHandler;

  // Tab close: the ×'s `nav/close` — same raw-wire seam as nav/select.
  const closeTabHandler: CloseTabHandler = async (target) => {
    await transport.execute(
      buildCommandUri('nav/close'),
      JSON.stringify({ userId: config.senderId, target }),
    );
  };
  widget.closeTabHandler = closeTabHandler;

  // Seed the who-panel directory from the core's own roster + the viewer —
  // independent of the per-room presence pipe: citizens (and YOU) visible
  // from the first frame, in every room.
  // Self-updating client: when a new build lands on the static server, the
  // page swaps itself — the operator never hand-refreshes to see a fix
  // (Joel, 2026-08-30: "i shouldnt have to refresh"). The dev server HMRs on
  // its own; this covers the preview/dist path. The current bundle name is in
  // the served index.html; a changed hash = a new build.
  const bundleOf = (html: string): string | undefined =>
    /assets\/index-[\w-]+\.js/.exec(html)?.[0];
  void (async () => {
    let current: string | undefined;
    try {
      current = bundleOf(await (await fetch('/', { cache: 'no-store' })).text());
    } catch {
      return; // non-static host (dev HMR) — nothing to watch
    }
    if (current === undefined) return;
    setInterval(() => {
      void (async () => {
        try {
          const served = bundleOf(await (await fetch('/', { cache: 'no-store' })).text());
          if (served !== undefined && served !== current) window.location.reload();
        } catch {
          /* server briefly away (rebuild) — next tick retries */
        }
      })();
    }, 15_000);
  })();

  // The viewer is ALWAYS in the directory, synchronously — a failed roster
  // fetch must never render the operator themself as absent/offline.
  widget.directorySeed = [
    {
      id: config.senderId,
      name: 'you',
      kind: 'human',
      active: true,
      runtime: 'interactive',
      vitals: {},
      lastSeenMs: Date.now(),
    },
  ];
  // Liveness re-polls: right after a core reboot the roster is empty while
  // citizens respawn — a one-shot seed would freeze that emptiness forever.
  // NOTE: `persona/roster` is the CANONICAL name — the auth gate does not yet
  // resolve aliases (persona/list bounces off the Owner wildcard; core fix
  // queued), so the canonical verb is load-bearing here.
  const seedDirectory = async (): Promise<void> => {
    // THE panel is global scope (Joel, 2026-08-31: "who all exists in this
    // continuum and who is online — any activity"). presence/directory is the
    // scope-wide daemon roster: every peer (humans, personas, external
    // agents) with heartbeat-real liveness + last-seen — the same answer in
    // every tab. Before it, only residents had a global source, so everyone
    // else's dot was "online for the tab".
    try {
      const raw = await transport.execute(buildCommandUri('presence/directory'), '{}');
      const parsed = JSON.parse(raw) as {
        peers?: readonly {
          name?: string;
          peer_id?: string;
          kind?: string;
          runtime?: string;
          online?: boolean;
          last_seen_ms?: number;
        }[];
      };
      const seed: RosterMemberVM[] = (parsed.peers ?? [])
        .filter((c) => c.peer_id)
        .map((c) => ({
          id: c.peer_id as string,
          name: c.name ?? (c.peer_id as string).slice(0, 8),
          kind: c.kind === 'human' ? 'human' : 'agent',
          active: c.online === true,
          runtime: c.runtime ?? '',
          vitals: {},
          lastSeenMs: c.last_seen_ms ?? 0,
        }));
      // The viewer stays first + always-online regardless of what the
      // directory knows about their peer row.
      const self = widget.directorySeed.filter((m) => m.id === config.senderId);
      widget.directorySeed = [...self, ...seed.filter((m) => m.id !== config.senderId)];
      widget.requestUpdate();
      return;
    } catch (err) {
      // Verb-absent (older core) → fall through to the residents-only seed;
      // anything else is a real failure worth the console.
      if (!/unknown command|no policy grants|not found/i.test(String(err))) {
        console.error('presence/directory seed failed:', err);
      }
    }
    try {
      const raw = await transport.execute(buildCommandUri('persona/roster'), '{}');
      const parsed = JSON.parse(raw) as {
        citizens?: readonly { agent_name?: string; peer_id?: string; resident?: boolean }[];
      };
      const seed: RosterMemberVM[] = (parsed.citizens ?? [])
        .filter((c) => c.peer_id)
        .map((c) => ({
          id: c.peer_id as string,
          name: c.agent_name ?? (c.peer_id as string).slice(0, 8),
          kind: 'agent',
          active: c.resident === true,
          runtime: '',
          vitals: {},
          lastSeenMs: 0,
        }));
      const self = widget.directorySeed.filter((m) => m.kind === 'human');
      widget.directorySeed = [...self, ...seed];
      widget.requestUpdate();
    } catch (err) {
      console.error('directory seed failed:', err);
    }
  };
  void seedDirectory();
  setInterval(() => void seedDirectory(), 30_000);

  // Round pause/resume from the bench rail (composed event out of the pure
  // renderer): bind to the AiSafe verbs. Fire-and-forget — the round's stage
  // flips in the next bench projection, so the UI's truth stays the feed,
  // never an optimistic local toggle.
  widget.addEventListener('bench-round-control', (e: Event) => {
    const { action, roundId } = (e as CustomEvent<{ action: string; roundId: string }>).detail;
    const verb = action === 'pause' ? 'benchmark/pause' : 'benchmark/resume';
    void transport
      .execute(buildCommandUri(verb), JSON.stringify({ roundId }))
      .catch((err: unknown) => console.error(`${verb} failed:`, err));
  });

  // CONNECTION STATE IS THE CONTINUON + FAVICON, never a text banner. The old
  // fixed bar ("positron: state feed reconnecting — … retry #19 in 8s") was the
  // anti-pattern this replaces (Joel, 2026-08-31: the continuon was designed to
  // indicate status ALONG with the favicon, dynamically). Humans read the orb
  // color and the tab icon; harnesses read `<html data-feed-status>`; engineers
  // read the console — each channel gets its own tier, none gets a banner.
  const favicon = ((): HTMLLinkElement => {
    const link = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
    if (link) return link;
    const fresh = document.createElement('link');
    fresh.rel = 'icon';
    document.head.appendChild(fresh);
    return fresh;
  })();
  const faviconHref = favicon.href; // the shipped mark, restored when live
  const FEED_TINT: Record<string, string> = {
    connecting: '#d8a53f',
    cached: '#d8a53f',
    reconnecting: '#d8a53f',
    closed: '#e5534b',
  };
  const setFavicon = (status: string): void => {
    const tint = FEED_TINT[status];
    if (tint === undefined) {
      favicon.href = faviconHref; // live — the real mark, untinted
      return;
    }
    const c = document.createElement('canvas');
    c.width = 64;
    c.height = 64;
    const ctx = c.getContext('2d');
    if (!ctx) return;
    ctx.beginPath();
    ctx.arc(32, 32, 22, 0, Math.PI * 2);
    ctx.fillStyle = tint;
    ctx.fill();
    favicon.href = c.toDataURL('image/png');
  };
  // Stamp the feed status SYNCHRONOUSLY before any await: from this instant the
  // page always carries `<html data-feed-status>`, so outside observers (shot
  // harness, e2e) wait on the app's own signal and can never mistake a
  // still-booting page for a settled one via the generic readyState fallback.
  document.documentElement.dataset.feedStatus = 'booting';
  setFavicon('connecting');

  // READ socket: subscribe to chat state, merge each envelope into the widget.
  // Durability + reconnection are POSITRON-inherent (StateConnection owns them,
  // adapter-driven): cached state paints instantly on boot, live envelopes
  // write through to IndexedDB, and a core reboot shows a visible
  // "reconnecting" status over last-known state — the app holds ZERO resilience
  // logic ([[one-logical-decision-one-place]]).
  let gotState = false;
  // The watchdog names the LAST feed status it saw, so a stuck boot says WHERE:
  // `none` = hydrate never finished; `connecting` = hydrate done, socket stuck.
  let lastFeedStatus = 'none';
  const state = new StateConnection(scopedWsUrl, undefined, {
    storage: new IndexedDbStateStorage(),
  });
  state.onStatus((status, detail) => {
    lastFeedStatus = status;
    // The feed status is FEEDBACK, so publish it where feedback belongs: one DOM
    // attribute on the root, not console spam. Anything outside the page — the
    // screenshot harness, an e2e test, a human in devtools — reads
    // `<html data-feed-status="live">` and knows the feed is E2E-healthy (a real
    // State frame landed; health is delivery, never socket existence — the
    // client-side twin of #280). This is the event the capture tooling waits on
    // instead of guessing with wall-clock (or worse, virtual-time) budgets.
    document.documentElement.dataset.feedStatus = status;
    // The three tiers: orb color (humans), favicon (the tab at a glance),
    // console (engineers — the only place the retry detail belongs).
    widget.feedStatus = status;
    setFavicon(status);
    if (status === 'live') {
      gotState = true;
      return;
    }
    if (detail) console.warn(`state feed ${status} — ${detail}`);
  });
  state.on(CHAT_KIND, (envelope: StateEnvelope) => {
    latest = chatStateFromEnvelope(envelope);
    widget.state = latest;
  });
  // The citizen's nav view (room set + unread) — per-user, served from THIS
  // session's ?me= scoped substrate. Upgrades the rooms rail from the single
  // focused room to the live room set as soon as the projector delivers.
  state.on(NAV_KIND, (envelope: StateEnvelope) => {
    const nav = navStateFromEnvelope(envelope);
    widget.nav = nav;
    resolveDeepLink(nav.open_tabs ?? []);
    // Pin the chat projection on first contact: with per-room presence
    // emitters live (#2606), an UNPINNED projection follows whichever room's
    // update lands next. One idempotent nav/select of the current tab arms
    // `pinned_away_from` so only explicit selection moves the view.
    if (!focusPinned && !pendingDeepLink) {
      const cur = nav.open_tabs?.find((t) => t.id === nav.current_tab);
      if (cur?.id !== undefined) {
        focusPinned = true;
        void selectRoomHandler(cur.id, 'room').catch(() => {
          focusPinned = false; // transient — retry on the next nav envelope
        });
      }
    }
    // The tab/window title mirrors the CURRENT activity — "continuum — cambriantech"
    // (Joel 2026-07-30; the #252 router's short-title rule, brand always lowercase).
    // App-level concern: index.ts owns the document, widgets never touch it.
    const current = nav.open_tabs?.find((t) => t.id === nav.current_tab);
    document.title = current?.title ? `continuum — ${current.title}` : 'continuum';
  });
  // The node's resource window (CPU/MEM) — the SYS gauge's core-carried series.
  state.on(SYSTEM_METRICS_KIND, (envelope: StateEnvelope) => {
    widget.sys = systemMetricsFromEnvelope(envelope);
  });
  // The serving glass box (#141) — model header + pager control-loop telemetry.
  state.on(SERVING_KIND, (envelope: StateEnvelope) => {
    widget.serving = servingFromEnvelope(envelope);
  });
  // The benchmark board (#329) — the academy rail's live run rows.
  state.on(BENCH_KIND, (envelope: StateEnvelope) => {
    widget.bench = benchFromEnvelope(envelope);
  });
  // The canvas feed (ninth ViewState, 2026-08-23) — the persona's own
  // observations of her artifact, published at the act seam: the desktop
  // watches the work itself.
  state.on(CANVAS_KIND, (envelope: StateEnvelope) => {
    widget.canvas = canvasFromEnvelope(envelope);
  });
  // The node's work board — the persona home's claims feed (cards by assignee).
  state.on(KANBAN_KIND, (envelope: StateEnvelope) => {
    widget.board = kanbanStateFromEnvelope(envelope);
  });
  // #170 live typing: grow a transient bubble per persona as its turn streams in.
  // Ephemeral — the durable message still arrives via the CHAT_KIND sink above, which
  // supersedes the bubble. The widget filters to its current room + retires on `done`.
  state.onStreamDelta((delta) => {
    widget.applyStreamDelta(delta);
  });
  // The boot watchdog is armed BEFORE the await, never after. A promise that
  // never settles inside `connect()` (storage hydrate, socket open) would
  // otherwise leave the banner frozen on the pre-connect string with no
  // diagnostic ever registered — the app cannot report the one failure that
  // stops it from reporting. Arming first makes a hung connect SAY SO, and
  // separating the two flags names WHICH half hung.
  let connectReturned = false;
  setTimeout(() => {
    if (gotState) return;
    // Engineer-tier diagnostic (console, never a banner): a hung boot SAYS
    // WHICH half hung. The user-facing signal is the amber orb + favicon.
    console.warn(
      connectReturned
        ? `positron: connected to ${config.wsUrl} but NO room snapshot arrived in 4s — subscribe/snapshot issue`
        : `positron: connect() has not returned after 4s (${config.wsUrl}) — last feed status: ${lastFeedStatus} (none=hydrate never finished, connecting=socket stuck)`,
    );
  }, 4000);
  // Connect: never throws with reconnect enabled — a dead core means cached
  // state + a loud `reconnecting` chip, and the SDK self-heals when it returns.
  await state.connect();
  connectReturned = true;
}

main().catch((err: unknown) => {
  // Boot failure (bad config, dead core) must be visible, not a blank page.
  console.error('web chat client failed to start:', err);
  const mount = document.getElementById('app') ?? document.body;
  const pre = document.createElement('pre');
  pre.style.cssText = 'padding:24px;color:#f77;font:13px/1.5 ui-monospace,monospace;white-space:pre-wrap';
  pre.textContent = `Continuum web chat failed to start:\n\n${err instanceof Error ? err.message : String(err)}`;
  mount.replaceChildren(pre);
});
