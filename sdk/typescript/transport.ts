/**
 * transport — the facade binding the typed SDK rides on.
 *
 * The substrate has exactly TWO primitives, each BIDIRECTIONAL (SDK-API-SURFACE.md):
 *   Commands = execute (call) + provide (serve)
 *   Events   = emit    (publish) + subscribe (listen)
 * => FOUR facade methods. The Rust FFI facade (`client/continuum-client-ffi`,
 * #1663) shipped `execute` + `subscribe`; `provide` + `emit` are the serve/publish
 * sides still being bound (flag to the facade owner — bind all four in one .udl
 * pass so the native binding is complete in one shot).
 *
 * This is the ONLY layer that touches bytes (JSON strings at the boundary). The
 * typed Commands/Events classes are thin generics over it; all logic is in the
 * Rust lib ([[headless-core-many-clients]]).
 */

/** The facade binding — wasm-bindgen(continuum-client) in browser, RustCoreIPC
 *  wire when the core is remote. JSON at the boundary; generic-free. */
export interface Transport {
  /** Command CALL: execute(command, params_json) -> result_json. (#1663) */
  execute(command: string, paramsJson: string): Promise<string>;
  /** Command SERVE: register a handler this client provides. (facade gap) */
  provide(command: string, handler: RawCommandHandler): Registration;
  /**
   * Event LISTEN: subscribe(topic, callback, filter?) -> Subscription. (#1663
   * shipped the bare 2-arg form; `filterJson` is the events-side refinement — the
   * redone AircEventPublisher applies it SERVER-SIDE via `matches_filter`, so
   * filtered-out events never cross the wire.) `topic` may be a bare class (local)
   * or an `airc://<peer>/events/<class>` address (a remote peer's events).
   */
  subscribe(topic: string, handlers: RawEventHandlers, filterJson?: string): Subscription;
  /** Event PUBLISH: emit(class, payload_json). (facade gap) */
  emit(eventClass: string, payloadJson: string): Promise<void>;
  /**
   * WHO this connection acts as — citizen (`userId`) + session instance
   * (`sessionId`), established by the binding at connect (airc pairing /
   * handshake; the spawn path for a persona). Readonly; the SDK surfaces it,
   * never fabricates it. Mirrors `continuum-client`'s `Connection::session()`.
   */
  session(): SessionIdentity;
}

/**
 * WHO a connection acts as — the first two ID tiers (CLAUDE.md: userId >
 * sessionId > contextId). Uuids are strings at the JSON boundary (the Rust
 * `Option<Uuid>` → `string | undefined`). The third tier, contextId, is NOT
 * here: it's per-scope, carried by a scoped client, not per-connection.
 */
export interface SessionIdentity {
  userId?: string;
  sessionId?: string;
}

/**
 * Stamp the conversation scope (`contextId`, the third ID tier) into an outbound
 * command envelope / event payload as a sibling field — the exact shape the core
 * reads it from (`command_envelope.rs`), mirroring `continuum-client`'s
 * `CommandClient::stamp_context`. Only objects are stamped (command params + event
 * payloads always are); a non-object is returned untouched rather than silently
 * wrapped. No-op when unscoped. Identity (userId/sessionId) is NEVER stamped here
 * — it's kernel-injected from the connection, which keeps identity unforgeable.
 */
export function stampContext<T>(value: T, contextId?: string): T {
  if (contextId == null) return value;
  if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
    return { ...(value as Record<string, unknown>), contextId } as T;
  }
  return value;
}

/** Raw (JSON-string) command handler — the facade's inbound-handler shape.
 *  A callback slot (function property, not a `this`-bound method) so a handler
 *  can be passed detached without losing binding. */
export interface RawCommandHandler {
  handle: (paramsJson: string) => Promise<string>;
}

/** Raw (JSON-string) event handlers — the facade's `EventCallback` shape. The
 *  `sequence` is the redone AircEventPublisher's monotonic per-subscription
 *  counter — lets subscribers detect ordering/gaps (multi-hop, any link emits). */
export interface RawEventHandlers {
  onEvent: (json: string, sequence: number) => void;
  onError?: (message: string) => void;
  onClosed?: () => void;
}

/** A live subscription; `unsubscribe()` tears it down (facade Drop). */
export interface Subscription {
  unsubscribe(): void;
}

/** A registered command provision; `remove()` deregisters it (facade Drop). */
export interface Registration {
  remove(): void;
}

/**
 * WHERE a command runs — the cross-environment dimension. Projects onto the redone
 * command addressing (`CommandUri`/`RouteDecision`, `core/src/routing/`):
 * `airc://[peer[@node]][:env]/path`. The SDK builds the URI; the core's
 * `RouteDecision` resolves local-walk / airc-to-peer / room fan-out (and, when
 * multi-hop lands, forwards across grid routers — no surface change).
 *
 * - omitted → Local (caller's own substrate; bare path, back-compatible).
 * - `{peer, node?, env?}` → a citizen; `env` = WHICH embodiment ('web'|'vr'|…).
 * - `{room, env?}` → fan-out to subscribers.
 * - `env: '*'` on a peer → every embodiment (broadcast).
 */
export type Target =
  | { peer: string; node?: string; env?: string }
  | { room: string; env?: string };

/** Build the redone airc:// command URI from a name + optional target. */
export function buildCommandUri(name: string, target?: Target): string {
  if (!target) return name; // Local — bare path
  if ('room' in target) {
    const env = target.env ? `:${target.env}` : '';
    return `airc://room:${target.room}${env}/${name}`;
  }
  const node = target.node ? `@${target.node}` : '';
  const env = target.env ? `:${target.env}` : '';
  return `airc://${target.peer}${node}${env}/${name}`;
}

/** Build the event topic address — bare class (local), or a SOURCE peer's/room's
 *  event topic across the grid: `airc://<peer|room>/events/<class>` (the redone
 *  AircEventPublisher subscribe path). Same destination-addressing as commands. */
export function buildEventTopic(eventClass: string, source?: Target): string {
  if (!source) return eventClass; // local events
  const authority = 'room' in source ? `room:${source.room}` : source.peer;
  return `airc://${authority}/events/${eventClass}`;
}
