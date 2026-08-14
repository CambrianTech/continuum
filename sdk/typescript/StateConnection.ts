/**
 * StateConnection — the thin-client's state-subscription primitive (task #29,
 * WIDGET-AS-STATE-KIND slice 3).
 *
 * The substrate has two primitives, each bidirectional: Commands (execute /
 * provide) and Events (emit / subscribe). Positron adds a THIRD wire flow the
 * command/event pair doesn't model — **state-down**: a consumer declares the
 * widget `kind`s it wants, and the substrate pushes a `ViewState` snapshot
 * immediately and every live change after, with no per-update request. That is
 * exactly what a renderer needs (a `<chat-widget>` mirroring a room's live
 * `ChatViewState`) and it is a poor fit for request/response `execute` or for
 * fire-and-forget `emit`. `StateConnection` is that flow's client.
 *
 * ## The wire is positron's, served by the core
 *
 * The core's WS ingress (`core/continuum-core/src/ipc/ws.rs`) routes a
 * `Subscribe` frame into a per-connection `run_session` and streams
 * `State(envelope)` frames back over the same socket — proven end-to-end by
 * `ws_subscribe_streams_a_live_state_frame_over_a_real_socket`. On the wire the
 * `Subscribe` client frame and the `State` server frame are byte-identical to
 * positron's own `ClientMessage::Subscribe` / `ServerMessage::State`
 * (continuum's `WsClientMessage` merely *projects onto* them), so this client
 * is typed against the vendored positron frames directly
 * (`./generated/positron`) — one source of truth for the state wire, no
 * hand-rolled shapes.
 *
 * ## One socket, state-only — for now
 *
 * This opens its OWN WebSocket carrying only Subscribe/State. Commands still
 * ride {@link WebSocketTransport}'s socket. The core multiplexes both flows on
 * one connection, so folding state into the command socket is a later
 * consolidation ([[lock-uniform-client-early]]); until then a read-only widget
 * needs only this, and a widget that also sends uses `Commands` alongside it.
 * The socket abstraction ({@link WebSocketLike}) is shared with
 * `WebSocketTransport` — one bytes-touching seam, injected for Node/tests.
 *
 * ## Neutral delivery — the SDK frames, the app projects
 *
 * A sink receives the raw {@link StateEnvelope} (`kind` + `revision` + `layer`
 * + `payload`). The SDK does NOT merge those into a positron `ViewState` or
 * assume a renderer: the app owns how `payload` (its own generated type, e.g.
 * `ChatViewState`) plus the envelope's `kind`/`revision` become the `ViewState`
 * a `LitHost` renders. Keeping the merge app-side is the compression rule — the
 * SDK holds zero view logic ([[headless-core-many-clients]]).
 */

import type { WebSocketLike, WebSocketCtor } from './WebSocketTransport';
import type { ClientMessage } from './generated/positron/ClientMessage';
import type { ServerMessage } from './generated/positron/ServerMessage';
import type { StateEnvelope } from './generated/positron/StateEnvelope';
import type { StateLayer } from './generated/positron/StateLayer';
import type { KindRevision } from './generated/positron/KindRevision';
import type { StateStorageAdapter } from './StateStorage';

/**
 * Receives successive {@link StateEnvelope}s for ONE widget `kind` — the
 * snapshot on subscribe, then every live change. The envelope's `payload` is
 * the consumer's own state type (`unknown` on the wire; the app narrows it);
 * `kind`/`revision`/`layer` frame it.
 */
export type StateSink = (envelope: StateEnvelope) => void;

/** A live per-kind registration; `off()` stops delivery to that sink. */
export interface StateSubscription {
  off(): void;
}

/**
 * One ephemeral token from a persona's in-progress turn (#170) — the live "typing"
 * surface, delivered token-by-token so a persona visibly types instead of freezing.
 * NOT part of the durable state contract: the authoritative message still arrives as
 * a `chat` {@link StateEnvelope}. Correlate to the eventual durable row by `roomId` +
 * `senderId` — the per-turn `streamId` is minted at stream start, NOT the final
 * message id. `done` marks the turn's end (retire the typing bubble).
 */
export interface StreamDelta {
  roomId: string;
  senderId: string;
  streamId: string;
  seq: number;
  token: string;
  done: boolean;
}

/** Receives successive {@link StreamDelta}s across all rooms/senders on this socket. */
export type StreamDeltaSink = (delta: StreamDelta) => void;

/**
 * The continuum-transport `stream_delta` frame — an ephemeral sibling of positron's
 * durable `ServerMessage`, carried on the same socket (fields are the wire's
 * snake_case, mirroring Rust `WsServerMessage::StreamDelta`). Kept local: this is
 * continuum's own transport frame, deliberately NOT folded into positron's
 * `ServerMessage` union so the durable state contract stays positron-pure.
 */
interface StreamDeltaFrame {
  type: 'stream_delta';
  room_id: string;
  sender_id: string;
  stream_id: string;
  seq: number;
  token: string;
  done: boolean;
}

/** Every frame this socket may receive: positron's durable state/response frames
 *  plus the continuum ephemeral token rail. */
type IncomingFrame = ServerMessage | StreamDeltaFrame;

/** Options for {@link StateConnection.connect}. */
export interface StateConnectOptions {
  /**
   * Cadence layers to receive. A renderer typically wants everything down to
   * `ephemeral`; an AI observer would ask for a coarser subset. Defaults to all
   * four layers (a renderer surface wants the liveliest cadence available).
   */
  layers?: StateLayer[];
}

/**
 * The feed's lifecycle, surfaced so a renderer can show ONE status chip and
 * otherwise keep painting last-known state (the Twitter model). Loud, never
 * silent: `reconnecting` is visible for as long as the core is away, so a
 * self-healing feed can never mask a persistently-dead core.
 *
 *   `cached`       — hydrated from local storage; live socket not yet up.
 *   `connecting`   — first connection attempt in flight.
 *   `live`         — connected and the state stream is flowing.
 *   `reconnecting` — feed dropped (or connect failed); retrying on a capped
 *                    backoff ladder. Last-known state stays on screen.
 *   `closed`       — the APP closed the feed intentionally; no retry.
 */
export type StateFeedStatus = 'cached' | 'connecting' | 'live' | 'reconnecting' | 'closed';

/** Notified on every {@link StateFeedStatus} transition. */
export type StateFeedStatusSink = (status: StateFeedStatus, detail?: string) => void;

/**
 * Construction options — the positron-inherent resilience contract. Durability
 * and reconnection live HERE, in the SDK, so every renderer (web, desktop,
 * mobile, headless observer) inherits them; apps never hand-roll caches or
 * retry loops ([[one-logical-decision-one-place]]).
 */
export interface StateConnectionOptions {
  /**
   * Durable local state (see `StateStorage`). When present: `connect()` first
   * hydrates every registered kind from cache (instant last-known paint, even
   * offline), and each live envelope is written through. Platform adapters:
   * `IndexedDbStateStorage` (browser), `MemoryStateStorage` (tests/ephemeral).
   */
  storage?: StateStorageAdapter;
  /**
   * Cache partition. Defaults to the connect URL — which carries `?me=<citizen>`,
   * so one citizen+endpoint never bleeds into another.
   */
  scope?: string;
  /**
   * Self-heal the feed (default true). The core reboots routinely; a dropped
   * socket retries on a capped backoff ladder (1s→10s) while the UI keeps
   * last-known state under a visible `reconnecting` status. Set false for
   * one-shot consumers (probes, tests) that want the legacy fail-loud connect.
   */
  reconnect?: boolean;
}

const ALL_LAYERS: StateLayer[] = ['ephemeral', 'session', 'persistent', 'semantic'];

/** Reconnect backoff ladder: 1s, 2s, 4s, 8s, then 10s forever. */
const reconnectDelayMs = (attempt: number): number =>
  Math.min(1000 * 2 ** Math.min(attempt - 1, 3), 10_000);

/**
 * How long the cache gets to answer before boot proceeds live-only.
 *
 * Generous against a real local read (IndexedDB answers in single-digit ms) and
 * far under any human patience threshold, because a MISSED cache costs only
 * first-paint latency — the substrate re-sends every kind's full snapshot on
 * subscribe, so nothing is lost but a head start.
 */
const HYDRATE_BUDGET_MS = 1_500;

/**
 * How long a WS handshake gets to complete (`open` or `error`) before the
 * attempt is treated as failed and handed to the retry ladder. Generous
 * against a healthy local core (measured 14 ms to first state envelope), and
 * strict enough that a silently-unresponsive ingress costs one visible retry,
 * never a hung boot.
 */
const OPEN_BUDGET_MS = 5_000;

/**
 * Resolve `work`, or REJECT at `budgetMs` — the piece a bare try/catch cannot
 * supply. `catch` covers a promise that rejects; a promise that never settles
 * has no failure to catch, so the awaiting code waits forever. Any await on an
 * external resource inside a boot path needs one of these.
 *
 * The timer is always cleared, so a slow-but-successful `work` leaves nothing
 * pending (a dangling handle would keep a node process alive at exit).
 */
async function withTimeout<T>(work: Promise<T>, budgetMs: number, message: string): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      work,
      new Promise<never>((_resolve, reject) => {
        timer = setTimeout(() => reject(new Error(message)), budgetMs);
      }),
    ]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

export class StateConnection {
  private readonly ctor: WebSocketCtor;
  private socket?: WebSocketLike;
  private connecting?: Promise<WebSocketLike>;
  /** One sink per widget kind. A second `on(kind, …)` replaces the first —
   *  the substrate serves ONE snapshot stream per kind, so one owner per kind
   *  keeps delivery unambiguous (the app composes multiple widgets of the same
   *  kind above this, not by double-subscribing the wire). */
  private readonly sinks = new Map<string, StateSink>();
  /** Latest revision seen per kind — replayed as `last_seen` on (re)connect so
   *  the substrate MAY skip a redundant snapshot the client already holds. */
  private readonly lastSeen = new Map<string, number>();
  private layers: StateLayer[] = ALL_LAYERS;
  /** Surfaced (not swallowed) when the socket drops — a dead feed means stale
   *  UI, which the app must be able to see and act on. */
  private onCloseCb?: (reason: string) => void;
  /** Sink for the ephemeral token rail (#170); undefined = no one is rendering
   *  live typing, so `stream_delta` frames are dropped (cheap, cosmetic). */
  private onStreamDeltaCb?: StreamDeltaSink;
  /** Positron-inherent resilience (see {@link StateConnectionOptions}). */
  private readonly storage?: StateStorageAdapter;
  private readonly scope: string;
  private readonly reconnect: boolean;
  private onStatusCb?: StateFeedStatusSink;
  private status?: StateFeedStatus;
  private hydrated = false;
  private reconnectAttempt = 0;
  private reconnectTimer?: ReturnType<typeof setTimeout>;
  /** True once the APP called close() — stops the retry ladder for good. */
  private closedIntentionally = false;

  /**
   * @param url the core's WS endpoint, e.g. `ws://127.0.0.1:<CONTINUUM_CORE_WS>`.
   * @param wsImpl optional WebSocket constructor; defaults to
   *   `globalThis.WebSocket` (browsers + Node ≥ 22). Fails loud at connect if
   *   neither is available — inject the `ws` package's client in that
   *   environment (same contract as {@link WebSocketTransport}).
   */
  constructor(
    private readonly url: string,
    wsImpl?: WebSocketCtor,
    options?: StateConnectionOptions,
  ) {
    const globalWs = (globalThis as { WebSocket?: WebSocketCtor }).WebSocket;
    const impl = wsImpl ?? globalWs;
    if (!impl) {
      throw new Error(
        'StateConnection: no WebSocket implementation available. Inject one ' +
          "(e.g. the 'ws' package's client) via the wsImpl constructor argument.",
      );
    }
    this.ctor = impl;
    this.storage = options?.storage;
    this.scope = options?.scope ?? url;
    this.reconnect = options?.reconnect ?? true;
  }

  /** Notified on every feed-status transition — render ONE chip from this. */
  onStatus(cb: StateFeedStatusSink): void {
    this.onStatusCb = cb;
    if (this.status) cb(this.status);
  }

  private setStatus(status: StateFeedStatus, detail?: string): void {
    if (this.status === status && !detail) return;
    this.status = status;
    this.onStatusCb?.(status, detail);
  }

  /**
   * Instant last-known paint: deliver each cached envelope to its registered
   * sink BEFORE the socket opens. Cached revisions are deliberately NOT fed
   * into `lastSeen` — the substrate still sends fresh snapshots on subscribe,
   * so the live stream always supersedes the cache (correctness first; the
   * skip-redundant-snapshot optimization can adopt cached revisions later).
   */
  private async hydrateFromStorage(): Promise<void> {
    if (!this.storage || this.hydrated) return;
    this.hydrated = true;
    try {
      // BOUNDED, not just try/caught. A `catch` covers a load that REJECTS; it
      // cannot cover one that never SETTLES, and a never-settling load is not
      // hypothetical — it took the whole web client down on 2026-08-14 (blank
      // page, banner frozen pre-connect, no diagnostic reachable because the
      // boot watchdog was armed after this await). Storage is declared an
      // ACCELERANT, never a dependency (see StateStorage.ts header); a bound is
      // what makes that declaration TRUE rather than aspirational. On timeout we
      // proceed live-only — the substrate re-sends full snapshots on subscribe,
      // so a skipped cache costs first-paint latency and nothing else.
      const rows = await withTimeout(
        this.storage.load(this.scope),
        HYDRATE_BUDGET_MS,
        `storage.load did not answer in ${HYDRATE_BUDGET_MS}ms`,
      );
      let delivered = 0;
      for (const row of rows) {
        const sink = this.sinks.get(row.envelope.kind);
        if (sink) {
          sink(row.envelope);
          delivered += 1;
        }
      }
      if (delivered > 0) this.setStatus('cached', `${delivered} kinds from cache`);
    } catch (err) {
      // Cache is an accelerant, never a dependency — log and proceed live-only.
      console.warn('StateConnection: hydrate from storage failed (continuing live-only):', err);
    }
  }

  /** Retry the feed on the capped ladder. Loud (status) and unstoppable until
   *  the app intentionally close()s or the socket comes back. */
  private scheduleReconnect(why: string): void {
    if (!this.reconnect || this.closedIntentionally || this.reconnectTimer) return;
    this.reconnectAttempt += 1;
    const delay = reconnectDelayMs(this.reconnectAttempt);
    this.setStatus('reconnecting', `${why} — retry #${this.reconnectAttempt} in ${Math.round(delay / 1000)}s`);
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = undefined;
      void (async () => {
        try {
          const socket = await this.ensureConnected();
          this.sendSubscribe(socket);
          // `live` is declared when the first state frame lands (onMessage),
          // not here — an open socket with no data is not "live".
        } catch (err) {
          this.scheduleReconnect(err instanceof Error ? err.message : String(err));
        }
      })();
    }, delay);
  }

  /**
   * Register the sink for a widget `kind`. Call before {@link connect} to be in
   * the initial subscription; calling after connect re-sends `Subscribe` with
   * the superset so the new kind starts streaming (snapshot-then-live) without
   * dropping the others.
   */
  on(kind: string, sink: StateSink): StateSubscription {
    this.sinks.set(kind, sink);
    // Already live → widen the subscription to include the new kind.
    if (this.socket) {
      this.sendSubscribe(this.socket);
    }
    return {
      off: () => {
        // Only remove if this exact sink still owns the kind (a later on() may
        // have replaced it).
        if (this.sinks.get(kind) === sink) {
          this.sinks.delete(kind);
        }
      },
    };
  }

  /** Notified when the socket closes (surface stale-feed to the app). */
  onClose(cb: (reason: string) => void): void {
    this.onCloseCb = cb;
  }

  /**
   * Register the sink for the live token rail (#170): a persona's in-progress turn
   * delivered token-by-token. Ephemeral — the durable message still arrives via the
   * `chat` state sink; this only drives the transient "typing" bubble. One sink (the
   * app fans out to the right room bubble by `delta.roomId`/`senderId`).
   */
  onStreamDelta(sink: StreamDeltaSink): void {
    this.onStreamDeltaCb = sink;
  }

  /**
   * Open the socket and subscribe to every registered `kind`. Resolves once the
   * subscription frame is sent (the snapshot arrives asynchronously via the
   * sinks, not this promise — the live `State` stream IS the acknowledgement,
   * so there is no per-subscribe reply to await). Fails loud if the connection
   * cannot be established.
   */
  async connect(options?: StateConnectOptions): Promise<void> {
    if (options?.layers) {
      if (options.layers.length === 0) {
        throw new Error(
          'StateConnection.connect: layers=[] would subscribe to nothing. Omit ' +
            'the option for all layers, or name the cadence layers you want.',
        );
      }
      this.layers = options.layers;
    }
    this.closedIntentionally = false;
    // Durable-first: cached state paints before the network is even attempted
    // (the Twitter model — instant UI, live reconciles). Awaited ONLY when an
    // adapter is present: without one, connect() constructs the socket
    // synchronously exactly as before (no gratuitous microtask boundary).
    if (this.storage) await this.hydrateFromStorage();
    if (this.status !== 'cached') this.setStatus('connecting');
    let socket: WebSocketLike;
    try {
      socket = await this.ensureConnected();
    } catch (err) {
      if (this.reconnect) {
        // Self-healing feed: a failed first connect (core still booting) rides
        // the same retry ladder as a drop. Cached state stays on screen under a
        // visible `reconnecting` status — resolve, don't throw.
        this.scheduleReconnect(err instanceof Error ? err.message : String(err));
        return;
      }
      throw err;
    }
    // Config errors (e.g. no registered kinds) stay FAIL-LOUD — they are the
    // caller's bug, not a network condition, and must never enter the retry ladder.
    this.sendSubscribe(socket);
  }

  /** Close the socket and STOP the retry ladder (intentional shutdown).
   *  Registered sinks are kept — a later {@link connect} re-subscribes them
   *  (with `last_seen` replay). */
  close(): void {
    this.closedIntentionally = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = undefined;
    }
    const socket = this.socket;
    this.socket = undefined;
    this.connecting = undefined;
    if (socket) socket.close();
    this.setStatus('closed');
  }

  private buildSubscribe(): ClientMessage {
    const last_seen: KindRevision[] = [];
    for (const [kind, revision] of this.lastSeen) {
      if (this.sinks.has(kind)) last_seen.push({ kind, revision });
    }
    return {
      type: 'subscribe',
      kinds: [...this.sinks.keys()],
      layers: this.layers,
      last_seen,
    };
  }

  private sendSubscribe(socket: WebSocketLike): void {
    if (this.sinks.size === 0) {
      throw new Error(
        'StateConnection: connect() with no registered kinds. Call on(kind, sink) ' +
          'before connect so there is something to subscribe to.',
      );
    }
    const frame = this.buildSubscribe();
    try {
      socket.send(JSON.stringify(frame));
    } catch (err) {
      throw new Error('StateConnection: failed to send subscribe frame', { cause: err });
    }
  }

  private ensureConnected(): Promise<WebSocketLike> {
    if (this.connecting) return this.connecting;

    this.connecting = new Promise<WebSocketLike>((resolve, reject) => {
      const socket = new this.ctor(this.url);
      // BOUNDED handshake — the hydrate lesson (2026-08-14), one seam over. A
      // socket that errors feeds the retry ladder; a socket that answers with
      // SILENCE (ingress accepted TCP but never completes the WS upgrade —
      // measured live: `last feed status: connecting`, boot hung, 1-in-5 boots)
      // fires neither `open` nor `error`, and an unbounded wait hangs connect()
      // forever. On timeout: reject like any handshake failure, so the same
      // ladder retries and the UI stays painted under a loud `reconnecting`.
      const openTimer = setTimeout(() => {
        this.connecting = undefined;
        reject(new Error(`StateConnection: ${this.url} did not complete the WS handshake in ${OPEN_BUDGET_MS}ms`));
        socket.close();
      }, OPEN_BUDGET_MS);
      socket.onopen = () => {
        clearTimeout(openTimer);
        this.socket = socket;
        resolve(socket);
      };
      socket.onerror = (ev) => {
        // Before open → connect failure; after open → onclose handles teardown.
        // Reject the handshake either way (a settled promise ignores a 2nd reject).
        clearTimeout(openTimer);
        this.connecting = undefined;
        reject(new Error(`StateConnection: connection to ${this.url} failed: ${String(ev)}`));
      };
      socket.onclose = () => {
        this.socket = undefined;
        this.connecting = undefined;
        // Surface, never swallow: a dropped feed leaves the UI stale. Then
        // self-heal — the retry ladder is LOUD (status stays `reconnecting`
        // the whole time the core is away), so recovery can never mask a
        // persistently-dead core; it just stops a routine core reboot from
        // orphaning every open tab.
        this.onCloseCb?.(`StateConnection: connection to ${this.url} closed`);
        this.scheduleReconnect('socket closed');
      };
      socket.onmessage = (ev) => { this.onMessage(ev.data); };
    });
    return this.connecting;
  }

  private onMessage(data: unknown): void {
    const text = typeof data === 'string' ? data : String(data);
    let msg: IncomingFrame;
    try {
      msg = JSON.parse(text) as IncomingFrame;
    } catch (err) {
      // A frame we can't parse has no kind to route to — loud, never silent.
      console.error('StateConnection: dropping unparseable server frame:', err);
      return;
    }

    // Exhaustive over IncomingFrame's union so a future server→client frame
    // surfaces as a compile error here rather than a silent drop.
    switch (msg.type) {
      case 'stream_delta': {
        // #170 ephemeral token rail — hand the app one live token; it grows the
        // matching sender's transient typing bubble. Never touches durable state.
        this.onStreamDeltaCb?.({
          roomId: msg.room_id,
          senderId: msg.sender_id,
          streamId: msg.stream_id,
          seq: msg.seq,
          token: msg.token,
          done: msg.done,
        });
        return;
      }
      case 'state': {
        // `{type:'state'} & StateEnvelope` — reconstruct the bare envelope for
        // the sink (drop the wire tag; the sink keys off `kind` itself).
        const envelope: StateEnvelope = {
          kind: msg.kind,
          revision: msg.revision,
          layer: msg.layer,
          payload: msg.payload,
        };
        if (envelope.revision !== undefined) {
          this.lastSeen.set(envelope.kind, envelope.revision);
        }
        // Live data is flowing — the feed is healthy; reset the retry ladder.
        this.reconnectAttempt = 0;
        this.setStatus('live');
        // Write-through: the durable cache always holds the newest snapshot per
        // kind (fire-and-forget — cache trouble must never stall the feed).
        if (this.storage) void this.storage.save(this.scope, envelope);
        const sink = this.sinks.get(envelope.kind);
        if (!sink) {
          // The substrate only sends kinds we subscribed to, so a kind with no
          // sink means one was removed mid-flight — report it, don't hide it.
          console.warn(
            `StateConnection: state for unsubscribed kind '${envelope.kind}' — dropping`,
          );
          return;
        }
        sink(envelope);
        return;
      }
      case 'command_failed': {
        // This client sends only Subscribe frames, which the substrate answers
        // with State (or silence), never CommandFailed. One here is a wiring
        // contradiction — surface loud, never fabricate a state update from it.
        console.error(
          `StateConnection: unexpected command_failed (${msg.correlation_id}) on a ` +
            `state-only socket: ${msg.error}`,
        );
        return;
      }
    }
  }
}
