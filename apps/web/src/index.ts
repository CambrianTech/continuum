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
  };
  widget.selectRoomHandler = selectRoomHandler;

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
  const historyHandler: HistoryHandler = async (roomId, beforeMessageId) => {
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

  // Visible connection diagnostics — a stuck "Connecting…" with no on-screen
  // reason is undebuggable. Surface the WS lifecycle so a blank/stuck tab tells
  // you WHY (socket closed / connected-but-no-snapshot / connect failed).
  const banner = document.createElement('div');
  banner.style.cssText =
    'position:fixed;top:0;left:0;right:0;z-index:9;padding:6px 12px;font:12px ui-monospace,monospace;background:#2a2a30;color:#cdcdd3;border-bottom:1px solid #3a3a42';
  const setStatus = (msg: string, warn = false): void => {
    banner.textContent = `positron: ${msg}`;
    banner.style.background = warn ? '#4a2a2a' : '#2a2a30';
    banner.style.color = warn ? '#f7b7b7' : '#cdcdd3';
    if (!banner.isConnected) document.body.appendChild(banner);
  };
  // Stamp the feed status SYNCHRONOUSLY before any await: from this instant the
  // page always carries `<html data-feed-status>`, so outside observers (shot
  // harness, e2e) wait on the app's own signal and can never mistake a
  // still-booting page for a settled one via the generic readyState fallback.
  document.documentElement.dataset.feedStatus = 'booting';
  setStatus(`connecting to ${config.wsUrl} …`);

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
    if (status === 'live') {
      banner.remove();
      gotState = true;
      return;
    }
    setStatus(`state feed ${status}${detail ? ` — ${detail}` : ''}`, status === 'reconnecting');
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
    setStatus(
      connectReturned
        ? `connected to ${config.wsUrl} but NO room snapshot arrived in 4s — subscribe/snapshot issue`
        : `connect() has not returned after 4s (${config.wsUrl}) — last feed status: ${lastFeedStatus} (none=hydrate never finished, connecting=socket stuck)`,
      true,
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
