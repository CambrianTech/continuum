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

import type { Transport, Target } from './transport';
import { Commands } from './Commands';
import { Events } from './Events';
import { Handle, handleFrom } from './Handle';
import type { CommandMap, CommandName } from './generated/CommandMap';

export class Continuum {
  /** Request/response + serve — the Command primitive. */
  readonly commands: Commands;
  /** Subscribe/emit — the Event primitive (the organism's signaling). */
  readonly events: Events;

  private constructor(private readonly transport: Transport) {
    this.commands = new Commands(transport);
    this.events = new Events(transport);
  }

  /**
   * Connect over a facade Transport. The platform SDK constructs the Transport
   * (wasm/wire/native); everything above it is identical across platforms.
   */
  static connect(transport: Transport): Continuum {
    return new Continuum(transport);
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
