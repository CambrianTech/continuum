/**
 * WebSocketTransport — the thin-client `Transport` binding over the core's
 * WebSocket ingress (`core/continuum-core/src/ipc/ws.rs`, task #29 Layer 0).
 *
 * This is the browser/mobile/TUI twin of the native FFI facade: the ONE layer
 * that touches bytes. A single socket **multiplexes** — N concurrent commands
 * ride one connection and replies are matched by a per-connection monotonic
 * correlation `id` (the [`WsClientMessage`]/[`WsServerMessage`] envelope). All
 * dispatch logic lives in the Rust core ([[headless-core-many-clients]]); this
 * class only frames, correlates, and unwraps.
 *
 * Wire types are the GENERATED mirrors of the Rust `continuum-airc-protocol`
 * envelope (`./generated/wire/transport/…`) — never hand-written here, so the
 * client can never drift from the server's wire shape ([[the-compression-principle]]).
 *
 * ## Scope today
 *
 * The WS ingress dispatches `Command` frames only, at the **Provisional
 * ceiling** (unauthenticated socket → AiSafe surface reachable, Owner-gated
 * commands refused at the dispatch boundary). So:
 *   - `execute` is live.
 *   - `provide` / `emit` / `subscribe` FAIL LOUD — the serve/publish/subscribe
 *     frames are later layers of task #29, not silently-swallowed no-ops.
 *   - `session()` returns `{}` (no identity established; a later GH-auth
 *     handshake raises the ceiling and populates it).
 */

import type {
  Transport,
  SessionIdentity,
  RawCommandHandler,
  RawEventHandlers,
  Registration,
  Subscription,
} from './transport';
import type { WsClientMessage } from './generated/wire/transport/WsClientMessage';
import type { WsServerMessage } from './generated/wire/transport/WsServerMessage';
import type { AircCommandRequest } from './generated/wire/transport/AircCommandRequest';

/** Route-kind literal for a command dispatched on the connected core (mirrors
 *  `continuum-airc-protocol`'s `KIND_PEER`). The thin client's core IS the peer:
 *  every frame is a local dispatch on it. */
const KIND_PEER = 'peer';

/**
 * Minimal structural WebSocket surface — enough to frame text and observe the
 * four lifecycle callbacks, without binding to the DOM `WebSocket` type. Lets a
 * Node consumer (`apps/mcp`) inject the `ws` package's client while the browser
 * uses the global. The `data` on a text frame is a string; a Node `Buffer`
 * stringifies cleanly.
 */
export interface WebSocketLike {
  send(data: string): void;
  close(): void;
  onopen: ((ev: unknown) => void) | null;
  onclose: ((ev: unknown) => void) | null;
  onerror: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
}

/** Constructs a {@link WebSocketLike} for a URL. `globalThis.WebSocket` and the
 *  `ws` package's client both satisfy this. */
export type WebSocketCtor = new (url: string) => WebSocketLike;

interface Pending {
  resolve: (resultJson: string) => void;
  reject: (err: Error) => void;
}

export class WebSocketTransport implements Transport {
  private readonly ctor: WebSocketCtor;
  private socket?: WebSocketLike;
  /** Cached connect handshake; cleared on close so a later `execute` reconnects
   *  (a dead core then fails loud on the fresh connect, never silently). */
  private connecting?: Promise<WebSocketLike>;
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();

  /**
   * @param url the core's WS endpoint, e.g. `ws://127.0.0.1:<CONTINUUM_CORE_WS>`.
   * @param wsImpl optional WebSocket constructor; defaults to `globalThis.WebSocket`
   *   (present in browsers and Node ≥ 22). Fails loud at connect if neither is
   *   available — inject the `ws` package's client in that environment.
   */
  constructor(
    private readonly url: string,
    wsImpl?: WebSocketCtor,
  ) {
    const globalWs = (globalThis as { WebSocket?: WebSocketCtor }).WebSocket;
    const impl = wsImpl ?? globalWs;
    if (!impl) {
      throw new Error(
        'WebSocketTransport: no WebSocket implementation available. Inject one ' +
          "(e.g. the 'ws' package's client) via the wsImpl constructor argument.",
      );
    }
    this.ctor = impl;
  }

  /** Command CALL: frame → send → await the correlated reply → unwrap. */
  async execute(command: string, paramsJson: string): Promise<string> {
    const socket = await this.ensureConnected();

    const request: AircCommandRequest = {
      path: command,
      kind: KIND_PEER,
      params: JSON.parse(paramsJson) as unknown,
    };
    const id = this.nextId++;
    const frame: WsClientMessage = { type: 'command', id, request };

    return await new Promise<string>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      try {
        socket.send(JSON.stringify(frame));
      } catch (err) {
        this.pending.delete(id);
        reject(new Error(`WebSocketTransport: send failed for '${command}': ${String(err)}`));
      }
    });
  }

  /** SERVE side — the WS ingress accepts Command frames only today. */
  provide(command: string, _handler: RawCommandHandler): Registration {
    throw new Error(
      `WebSocketTransport: provide('${command}') is not supported — the WS ingress ` +
        'dispatches Command frames only (serve is a later task #29 layer).',
    );
  }

  /** Event PUBLISH — not carried by the WS ingress yet. */
  emit(eventClass: string, _payloadJson: string): Promise<void> {
    return Promise.reject(
      new Error(
        `WebSocketTransport: emit('${eventClass}') is not supported — the WS ingress ` +
          'carries no event-publish frame yet (a later task #29 layer).',
      ),
    );
  }

  /** Event LISTEN — not carried by the WS ingress yet. */
  subscribe(topic: string, _handlers: RawEventHandlers, _filterJson?: string): Subscription {
    throw new Error(
      `WebSocketTransport: subscribe('${topic}') is not supported — the WS ingress ` +
        'carries no event-delivery frame yet (a later task #29 layer).',
    );
  }

  /** Unauthenticated Provisional socket — no identity established. A later
   *  GH-auth handshake populates this. Never fabricated. */
  session(): SessionIdentity {
    return {};
  }

  /** Close the socket and reject any in-flight commands. */
  close(): void {
    const socket = this.socket;
    this.socket = undefined;
    this.connecting = undefined;
    if (socket) socket.close();
    this.failPending(new Error('WebSocketTransport: closed by caller'));
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
        // Before open, this is a connect failure; after open, the close handler
        // does the pending cleanup. Reject the handshake either way — a settled
        // promise ignores a second reject.
        this.connecting = undefined;
        reject(new Error(`WebSocketTransport: connection to ${this.url} failed: ${String(ev)}`));
      };
      socket.onclose = () => {
        this.socket = undefined;
        this.connecting = undefined;
        this.failPending(new Error(`WebSocketTransport: connection to ${this.url} closed`));
      };
      socket.onmessage = (ev) => { this.onMessage(ev.data); };
    });
    return this.connecting;
  }

  private onMessage(data: unknown): void {
    const text = typeof data === 'string' ? data : String(data);
    let msg: WsServerMessage;
    try {
      msg = JSON.parse(text) as WsServerMessage;
    } catch (err) {
      // A frame we can't parse carries no correlation id — nothing to resolve.
      // Loud, never silent.
      console.error('WebSocketTransport: dropping unparseable server frame:', err);
      return;
    }

    // `WsServerMessage` is single-variant (`response`) today — a runtime switch on
    // `msg.type` would be a comparison that is always true. The future-proofing is
    // therefore a COMPILE-TIME guard: this destructure reads `id`/`response` off
    // the frame, so the day a second server→client variant (e.g. event delivery)
    // is added to the union — one that lacks those fields — this line stops
    // compiling and forces a real dispatch here rather than a silent drop.
    const { id, response } = msg;
    // A frame with NO id is not a reply — the ingress also fans out push-style
    // frames (state envelopes / stream deltas) to every connection, including
    // command-only sockets (glass-boxed live 2026-07-30: 22× "unknown
    // correlation id undefined" spam while every command actually worked).
    // Ignore pushes here — the StateConnection socket is their consumer; the
    // server-side fix is not fanning out to unsubscribed sockets at all.
    if (id === undefined || id === null) return;
    const pending = this.pending.get(id);
    if (!pending) {
      console.error(`WebSocketTransport: reply for unknown correlation id ${id}`);
      return;
    }
    this.pending.delete(id);
    if (response.status === 'ok') {
      pending.resolve(JSON.stringify(response.result));
    } else {
      pending.reject(new Error(response.message));
    }
  }

  private failPending(err: Error): void {
    for (const pending of this.pending.values()) {
      pending.reject(err);
    }
    this.pending.clear();
  }
}
