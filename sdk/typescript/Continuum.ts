/**
 * Continuum — the SDK entry point. THIS is how the SDKs work: one object over the
 * facade Transport exposing the two primitives (commands + events) and the handle
 * pattern. Every per-platform SDK (web/desktop, and the native swift/kotlin/flutter
 * twins) presents this same shape — only the Transport binding differs
 * (wasm-bindgen, RustCoreIPC wire, or the native uniffi facade).
 *
 * Zero logic ([[headless-core-many-clients]]): it wires Commands + Events + Handle
 * over the Transport. All behavior is in the Rust lib; this is the idiomatic skin.
 * See docs/architecture/{CLIENT-SDK-PLATFORM-ARCHITECTURE, SDK-API-SURFACE}.md.
 */

import type { Transport, Target, SessionIdentity } from './transport';
import { Commands } from './Commands';
import { Events } from './Events';
import { Handle, handleFrom } from './Handle';
import type { CommandMap, CommandName } from './generated/CommandMap';

export class Continuum {
  /** Request/response + serve — the Command primitive. */
  readonly commands: Commands;
  /** Subscribe/emit — the Event primitive (the organism's signaling). */
  readonly events: Events;

  private constructor(
    private readonly transport: Transport,
    private readonly contextId?: string,
  ) {
    this.commands = new Commands(transport, contextId);
    this.events = new Events(transport, contextId);
  }

  /**
   * Connect over a facade Transport. The platform SDK constructs the Transport
   * (wasm/wire/native); everything above it is identical across platforms.
   */
  static connect(transport: Transport): Continuum {
    return new Continuum(transport);
  }

  /**
   * WHO this client acts as — citizen (`userId`) + session instance
   * (`sessionId`). Readonly; surfaces the identity established at connect (airc
   * pairing / handshake, or the persona's own id). Mirrors `continuum-client`'s
   * `Connection::session()` — the same shape every client and persona reads.
   */
  get session(): SessionIdentity {
    return this.transport.session();
  }

  /** The conversation/room this client is scoped to, if any (third ID tier). */
  get context(): string | undefined {
    return this.contextId;
  }

  /**
   * Return a client SCOPED to a conversation/room — its `commands` + `events`
   * auto-stamp `contextId` (the third ID tier) so callers never re-thread the
   * scope. Shares the same Transport + identity; only the context differs. This
   * is how a persona services a room (scoped to that room's contextId) exactly
   * the way a browser tab does — `[[persona-is-a-client]]`.
   */
  scoped(contextId: string): Continuum {
    return new Continuum(this.transport, contextId);
  }

  /**
   * Open a long-running / streaming resource and get an addressable {@link Handle}.
   * The open-style command's result carries the resource's `airc://` URI (result
   * body or airc Location-style header); the Handle routes further ops + its event
   * stream to that URI — wherever the resource lives on the grid. This is the
   * WebRTC-connection / file-session / inference-session shape.
   */
  async open<K extends CommandName>(
    name: K,
    params: CommandMap[K]['params'],
    target?: Target,
  ): Promise<Handle> {
    const result = (await this.commands.execute(name, params, target)) as { uri: string };
    return handleFrom(result.uri, this.transport);
  }
}
