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

const ALL_LAYERS: StateLayer[] = ['ephemeral', 'session', 'persistent', 'semantic'];

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
    const socket = await this.ensureConnected();
    this.sendSubscribe(socket);
  }

  /** Close the socket. Registered sinks are kept — a later {@link connect}
   *  re-subscribes them (with `last_seen` replay). */
  close(): void {
    const socket = this.socket;
    this.socket = undefined;
    this.connecting = undefined;
    if (socket) socket.close();
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
      socket.onopen = () => {
        this.socket = socket;
        resolve(socket);
      };
      socket.onerror = (ev) => {
        // Before open → connect failure; after open → onclose handles teardown.
        // Reject the handshake either way (a settled promise ignores a 2nd reject).
        this.connecting = undefined;
        reject(new Error(`StateConnection: connection to ${this.url} failed: ${String(ev)}`));
      };
      socket.onclose = () => {
        this.socket = undefined;
        this.connecting = undefined;
        // Surface, never swallow: a dropped feed leaves the UI stale. The app
        // decides whether to reconnect (StateConnection does not auto-reconnect
        // — a silent reconnect would mask a persistently-dead core).
        this.onCloseCb?.(`StateConnection: connection to ${this.url} closed`);
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
