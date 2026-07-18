/**
 * NodeSocketTransport — the `Transport` binding over the core's **Unix-domain /
 * TCP IPC socket** (`core/continuum-core/src/ipc/mod.rs`), the transport that
 * carries the SERVE side (`provide`) the `WebSocketTransport` cannot.
 *
 * This is the task-#29 "serve layer": a client that not only CALLS the core but
 * PROVIDES capabilities the core routes back to it — the eye-node that fulfils
 * `perception/observe` / `interface/screenshot`. The core's provider seam
 * (`ipc/provider_bridge.rs`) forwards a persona's Provided command down this
 * socket as a `provideCall` frame; this transport dispatches it against the
 * `provide()` registrations and replies with a `provideResult`.
 *
 * ## Wire protocol (mirrors the Rust IPC server)
 *
 * - **Client → core** (this side writes): newline-delimited JSON.
 *   - a command request: `{ command, ...params, requestId }` (requestId lets us
 *     correlate concurrent replies).
 *   - a provider handshake: the `provider/register` command above.
 *   - a back-channel reply: `{ type: "provideResult", callId, success, result?, error? }`.
 * - **Core → client** (this side reads): length-prefixed frames
 *   `[u32 BE length][payload]`, payload optionally `json\0binary`. Two shapes:
 *   - a command response: `{ success, result?, error?, requestId }`.
 *   - a core-initiated call: `{ type: "provideCall", callId, command, params }`.
 *
 * ## Runtime-agnostic by construction
 *
 * Like {@link WebSocketTransport}, this class does NOT hard-import `net`/`Buffer`
 * — the substrate SDK stays free of any single runtime ([[headless-core-many-clients]]).
 * The consumer injects a {@link SocketConnector} (in Node: `() =>
 * net.createConnection(path)`); framing uses `Uint8Array`/`DataView`/`TextDecoder`,
 * present in every modern runtime.
 */

import type {
  Transport,
  SessionIdentity,
  RawCommandHandler,
  RawEventHandlers,
  Registration,
  Subscription,
} from './transport';

/**
 * Minimal duplex-socket surface — enough to write text frames and observe the
 * connect / data / error / close lifecycle, without binding to Node's
 * `net.Socket` type. A Node `net.Socket` satisfies it (its `data` chunk is a
 * `Buffer`, which IS a `Uint8Array`).
 */
export interface DuplexSocketLike {
  write(data: string, cb?: (err?: Error | null) => void): void;
  end(): void;
  on(event: 'data', listener: (chunk: Uint8Array) => void): unknown;
  on(event: 'connect', listener: () => void): unknown;
  on(event: 'error', listener: (err: Error) => void): unknown;
  on(event: 'close', listener: () => void): unknown;
}

/** Opens a fresh {@link DuplexSocketLike} to the core. In Node:
 *  `() => net.createConnection(socketPathOrTcp)`. */
export type SocketConnector = () => DuplexSocketLike;

interface PendingCall {
  resolve: (resultJson: string) => void;
  reject: (err: Error) => void;
}

/** The core→client back-channel frame this transport dispatches. */
interface ProvideCallFrame {
  type: 'provideCall';
  callId: number;
  command: string;
  params: unknown;
}

export class NodeSocketTransport implements Transport {
  private socket?: DuplexSocketLike;
  private connecting?: Promise<DuplexSocketLike>;
  private nextRequestId = 1;
  private readonly pending = new Map<number, PendingCall>();

  /** Registered provider handlers, keyed by exact command name. */
  private readonly handlers = new Map<string, RawCommandHandler>();
  /** Provided-but-not-yet-registered command names (flushed by {@link flush}). */
  private readonly unregistered: string[] = [];

  /** Inbound read buffer for length-prefixed frame reassembly. */
  private buffer = new Uint8Array(0);
  private readonly decoder = new TextDecoder();

  /**
   * @param connect opens the socket (inject the runtime's connector).
   * @param label human name this client registers under (shown in the core's
   *   provider logs, e.g. `"eye-node@laptop-3"`). Defaults to `"node-client"`.
   */
  constructor(
    private readonly connect: SocketConnector,
    private readonly label: string = 'node-client',
  ) {}

  /** Command CALL: frame → send → await the reply correlated by requestId. */
  async execute(command: string, paramsJson: string): Promise<string> {
    const socket = await this.ensureConnected();
    const params = paramsJson ? (JSON.parse(paramsJson) as Record<string, unknown>) : {};
    const requestId = this.nextRequestId++;
    // The Rust reader spreads params at the top level (`{ command, ...params }`),
    // so match that exactly — it reads e.g. `target` off the request object.
    const frame = JSON.stringify({ command, ...params, requestId }) + '\n';

    return await new Promise<string>((resolve, reject) => {
      this.pending.set(requestId, { resolve, reject });
      socket.write(frame, (err) => {
        if (err) {
          this.pending.delete(requestId);
          reject(new Error(`NodeSocketTransport: send failed for '${command}': ${String(err)}`));
        }
      });
    });
  }

  /**
   * SERVE side — register a capability this client provides. The handler is
   * stored locally and the command name is queued for a `provider/register`
   * handshake sent by {@link flush} (call it once after all `provide`s). The
   * core then routes matching persona calls back here as `provideCall` frames.
   *
   * `remove()` drops the LOCAL handler (a later call then fails loud "no
   * handler"); a full core-side deregister verb is a later refinement.
   */
  provide(command: string, handler: RawCommandHandler): Registration {
    this.handlers.set(command, handler);
    this.unregistered.push(command);
    return {
      remove: () => {
        this.handlers.delete(command);
      },
    };
  }

  /**
   * Send the `provider/register` handshake for every command `provide`d since
   * the last flush, binding them in the core's `ProviderRegistry`. Awaitable so
   * the caller knows the core will now route to it. Fails loud if the core
   * rejects a registration (e.g. a non-Provided command).
   */
  async flush(): Promise<void> {
    if (this.unregistered.length === 0) return;
    const commands = this.unregistered.splice(0, this.unregistered.length);
    // execute() throws on a rejected response — a rejected register surfaces.
    await this.execute(
      'provider/register',
      JSON.stringify({ commands, label: this.label }),
    );
  }

  /** Event PUBLISH — not carried on this transport yet (later #29 layer). */
  emit(eventClass: string, _payloadJson: string): Promise<void> {
    return Promise.reject(
      new Error(
        `NodeSocketTransport: emit('${eventClass}') is not supported yet — the IPC ` +
          'socket carries commands + the provider back-channel, not event publish.',
      ),
    );
  }

  /** Event LISTEN — not carried on this transport yet (later #29 layer). */
  subscribe(topic: string, _handlers: RawEventHandlers, _filterJson?: string): Subscription {
    throw new Error(
      `NodeSocketTransport: subscribe('${topic}') is not supported yet — the IPC ` +
        'socket carries commands + the provider back-channel, not event delivery.',
    );
  }

  /** Local owner-by-locality socket — identity isn't established by a handshake
   *  here, so this is honestly empty (never fabricated). */
  session(): SessionIdentity {
    return {};
  }

  /** Close the socket and reject any in-flight calls. */
  close(): void {
    const socket = this.socket;
    this.socket = undefined;
    this.connecting = undefined;
    if (socket) socket.end();
    this.failPending(new Error('NodeSocketTransport: closed by caller'));
  }

  private ensureConnected(): Promise<DuplexSocketLike> {
    if (this.socket) return Promise.resolve(this.socket);
    if (this.connecting) return this.connecting;

    this.connecting = new Promise<DuplexSocketLike>((resolve, reject) => {
      const socket = this.connect();
      socket.on('connect', () => {
        this.socket = socket;
        resolve(socket);
      });
      socket.on('data', (chunk) => {
        this.onData(chunk);
      });
      socket.on('error', (err) => {
        this.connecting = undefined;
        reject(new Error(`NodeSocketTransport: connection failed: ${err.message}`));
      });
      socket.on('close', () => {
        this.socket = undefined;
        this.connecting = undefined;
        this.failPending(new Error('NodeSocketTransport: connection closed'));
      });
    });
    return this.connecting;
  }

  /** Reassemble length-prefixed frames from the byte stream and dispatch each. */
  private onData(chunk: Uint8Array): void {
    const merged = new Uint8Array(this.buffer.length + chunk.length);
    merged.set(this.buffer, 0);
    merged.set(chunk, this.buffer.length);
    this.buffer = merged;

    while (this.buffer.length >= 4) {
      const view = new DataView(this.buffer.buffer, this.buffer.byteOffset, this.buffer.length);
      const length = view.getUint32(0, false); // big-endian
      const frameEnd = 4 + length;
      if (this.buffer.length < frameEnd) break;

      const payload = this.buffer.subarray(4, frameEnd);
      this.buffer = this.buffer.subarray(frameEnd);

      // A binary frame is `json\0rawbytes`; we only speak JSON here, so take the
      // JSON prefix (the whole payload when there's no separator).
      const sep = payload.indexOf(0);
      const jsonBytes = sep !== -1 ? payload.subarray(0, sep) : payload;

      let obj: unknown;
      try {
        obj = JSON.parse(this.decoder.decode(jsonBytes));
      } catch (err) {
        console.error('NodeSocketTransport: dropping unparseable frame:', err);
        continue;
      }
      this.handleFrame(obj);
    }
  }

  private handleFrame(obj: unknown): void {
    const frame = obj as Record<string, unknown>;

    // Core→client call: fulfil it against a provider registration.
    if (frame.type === 'provideCall') {
      void this.dispatchProvide(frame as unknown as ProvideCallFrame);
      return;
    }

    // Otherwise a response to one of our requests.
    const requestId = frame.requestId;
    if (typeof requestId !== 'number') {
      console.error('NodeSocketTransport: frame with no requestId:', obj);
      return;
    }
    const pending = this.pending.get(requestId);
    if (!pending) {
      console.error(`NodeSocketTransport: reply for unknown requestId ${requestId}`);
      return;
    }
    this.pending.delete(requestId);
    if (frame.success) {
      pending.resolve(JSON.stringify(frame.result ?? null));
    } else {
      pending.reject(new Error(typeof frame.error === 'string' ? frame.error : 'command failed'));
    }
  }

  /** Run a provider handler for a core-initiated call and reply on the socket. */
  private async dispatchProvide(frame: ProvideCallFrame): Promise<void> {
    const socket = this.socket;
    if (!socket) return; // connection gone; the core times out and fails loud

    const handler = this.handlers.get(frame.command);
    if (!handler) {
      this.writeProvideResult(socket, frame.callId, {
        success: false,
        error: `this client provides no handler for '${frame.command}'`,
      });
      return;
    }
    try {
      const resultJson = await handler.handle(JSON.stringify(frame.params ?? {}));
      this.writeProvideResult(socket, frame.callId, {
        success: true,
        result: JSON.parse(resultJson),
      });
    } catch (err) {
      this.writeProvideResult(socket, frame.callId, {
        success: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  private writeProvideResult(
    socket: DuplexSocketLike,
    callId: number,
    body: { success: boolean; result?: unknown; error?: string },
  ): void {
    socket.write(JSON.stringify({ type: 'provideResult', callId, ...body }) + '\n');
  }

  private failPending(err: Error): void {
    for (const pending of this.pending.values()) {
      pending.reject(err);
    }
    this.pending.clear();
  }
}
