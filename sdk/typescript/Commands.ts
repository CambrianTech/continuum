/**
 * Commands — the elegant typed command surface for the TypeScript SDK.
 *
 * The whole SDK is two primitives over the headless Rust core (the FFI facade,
 * `client/continuum-client-ffi`): request/response (`execute`) and pub/sub
 * (`subscribe`). See docs/architecture/SDK-API-SURFACE.md.
 *
 * The win over the old `execute<T extends CommandParams, U extends CommandResult>`
 * (caller supplies the generics): here both the params type AND the result type are
 * INFERRED from the command-name literal via the generated `CommandMap`. The caller
 * writes only the name + params and gets a fully-typed result — no `<T,U>`.
 *
 * Zero logic lives here ([[headless-core-many-clients]] organizing law): this is a
 * thin typed wrapper over the facade. All behavior is in the Rust lib.
 *
 * NOTE: `CommandMap` / `EventMap` are GENERATED (one entry per discovered command,
 * params/result = the ts-rs wire types in protocol/typescript/*). They are NOT a
 * hand-maintained registry — regenerated on every command change, so the elegance
 * stays compatible with the no-hardcoded-registry rule (CLAUDE.md).
 */

import type { CommandMap, CommandName, EventMap, EventClass } from './generated/CommandMap';

/**
 * The facade binding — the low-level JSON-at-the-boundary transport the typed
 * layer rides on. Backed by wasm-bindgen(continuum-client) in the browser, or the
 * RustCoreIPC wire when the core is remote. The SDK is constructed with one; it is
 * the ONLY thing that touches bytes.
 */
export interface CommandTransport {
  /** Mirrors the Rust facade: execute(command, params_json) -> result_json. */
  execute(command: string, paramsJson: string): Promise<string>;
  /** Mirrors the Rust facade: subscribe(class, callback) -> Subscription. */
  subscribe(eventClass: string, handlers: RawEventHandlers): Subscription;
  /**
   * PROVIDE: register a handler this client implements, so the core (or a peer)
   * can route the command HERE. The third primitive — commands are bidirectional:
   * a client both CALLS commands (execute) and PROVIDES them. Client-provided
   * commands (`interface/screenshot`, capture, `ping`) have a rust-origin CONTRACT
   * (name + ts-rs types) but a per-PLATFORM adapter implementation that only the
   * client can run — web = DOM/canvas, desktop = OS, AR/VR = capture from the
   * renderer. The core can't screenshot the client's display; the client must.
   * Mirrors the persona `command_inbound_pump` (server-side) at the SDK boundary.
   * Needs a facade primitive `provide(command, handler) -> Registration`.
   */
  provide(command: string, handler: RawCommandHandler): Registration;
}

/** Raw (JSON-string) command handler — the facade's inbound-handler shape. */
export interface RawCommandHandler {
  /** Run the client-side adapter for this command; return serialized result. */
  handle(paramsJson: string): Promise<string>;
}

/** A registered command provision; dropping it removes the handler. */
export interface Registration {
  remove(): void;
}

/** Raw (JSON-string) event handlers — the facade's `EventCallback` shape. */
export interface RawEventHandlers {
  onEvent(json: string): void;
  onError?(message: string): void;
  onClosed?(): void;
}

/** A live subscription; dropping it (calling `unsubscribe`) tears it down. */
export interface Subscription {
  unsubscribe(): void;
}

/** Typed event handlers — payload is the generated type for the class. */
export interface EventHandlers<K extends EventClass> {
  onEvent(event: EventMap[K]): void;
  onError?(message: string): void;
  onClosed?(): void;
}

/**
 * The typed command surface. One instance per connection; holds only the
 * transport. Every method is a typed projection over `execute`/`subscribe`.
 */
export class Commands {
  constructor(private readonly transport: CommandTransport) {}

  /**
   * Execute a command. The name literal infers the params shape and the result
   * type — `execute('data/list', { collection })` returns `Promise<DataListResult>`.
   */
  async execute<K extends CommandName>(
    name: K,
    params: CommandMap[K]['params'],
  ): Promise<CommandMap[K]['result']> {
    const resultJson = await this.transport.execute(name, JSON.stringify(params));
    // The facade guarantees result_json is the serialized result type; the
    // generated map guarantees the static type. One cast at the boundary, typed
    // everywhere above it.
    return JSON.parse(resultJson) as CommandMap[K]['result'];
  }

  /**
   * Subscribe to an event class. The class literal infers the payload type, so
   * `subscribe('data:chat_messages:created', { onEvent: m => … })` types `m` as the
   * generated event payload.
   */
  subscribe<K extends EventClass>(eventClass: K, handlers: EventHandlers<K>): Subscription {
    return this.transport.subscribe(eventClass, {
      onEvent: (json) => handlers.onEvent(JSON.parse(json) as EventMap[K]),
      onError: handlers.onError,
      onClosed: handlers.onClosed,
    });
  }

  /**
   * PROVIDE a command this client implements — the platform adapter for a
   * client-provided command (screenshot, capture, ping). The contract is
   * rust-origin (params/result inferred from the name via `CommandMap`); the
   * implementation is this platform's adapter. When the core routes the command
   * here, `adapter` runs and its typed result is serialized back.
   *
   *   commands.provide('interface/screenshot', async (p) => webCapture(p));   // web
   *   commands.provide('interface/screenshot', async (p) => rendererCapture(p)); // AR/VR
   *
   * Same command identity, N platform adapters (OpenCV-style adapter polymorphism).
   */
  provide<K extends CommandName>(
    name: K,
    adapter: (params: CommandMap[K]['params']) => Promise<CommandMap[K]['result']>,
  ): Registration {
    return this.transport.provide(name, {
      handle: async (paramsJson) => {
        const result = await adapter(JSON.parse(paramsJson) as CommandMap[K]['params']);
        return JSON.stringify(result);
      },
    });
  }
}
