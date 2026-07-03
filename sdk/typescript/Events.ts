/**
 * Events — the typed Event primitive (EMIT + SUBSCRIBE) for the TS SDK.
 *
 * Same depth as Commands: subscribe to a SOURCE (local, or a peer's/room's events
 * across the grid — same airc:// addressing), with SERVER-SIDE filtering (the
 * redone AircEventPublisher's `matches_filter` — filtered events never cross the
 * wire), and a monotonic SEQUENCE per subscription (ordering/gap detection; in a
 * multi-hop chain any link can emit, so ordering matters). Events = emit + subscribe.
 *
 * Zero logic ([[headless-core-many-clients]]): thin typed wrapper over the facade
 * `Transport`. `EventMap` generated. See docs/architecture/SDK-API-SURFACE.md.
 */

import type { Transport, Subscription, Target } from './transport';
import { buildEventTopic, stampContext } from './transport';
import type { EventMap, EventClass } from './generated/EventMap';

// `Subscription` is the public return type of `subscribe`, so it belongs to the
// Events surface — re-export it here. The generated `EventApi` (sdk_codegen/events.rs)
// imports it from `../Events`, so this is the single source that contract binds to.
export type { Subscription } from './transport';

/** Metadata delivered alongside each event (from the redone publisher frame). */
export interface EventMeta {
  /** Monotonic per-subscription sequence — detect ordering/gaps. */
  sequence: number;
}

/** Typed event handlers — payload + meta typed from the class. */
export interface EventHandlers<K extends EventClass> {
  onEvent(event: EventMap[K], meta: EventMeta): void;
  onError?(message: string): void;
  onClosed?(): void;
}

/** Where the events come FROM (omitted = local), and an optional server-side
 *  filter (a partial match over the payload; richer predicates pass through). */
export interface SubscribeOptions<K extends EventClass> {
  source?: Target;
  filter?: Partial<EventMap[K]>;
}

export class Events {
  /**
   * @param transport the facade binding
   * @param contextId the conversation/room scope (set by `Continuum.scoped(ctx)`);
   *   when present, `emit` stamps it into the event payload so an emitted event
   *   carries the conversation it belongs to. Mirrors `continuum-client`.
   */
  constructor(
    private readonly transport: Transport,
    private readonly contextId?: string,
  ) {}

  /**
   * SUBSCRIBE to an event class. The class literal infers the payload type.
   * `source` addresses whose events (a peer's/room's, across the grid); `filter`
   * is applied SERVER-SIDE so non-matching events never cross the wire.
   *
   *   events.subscribe('contract:proposed', { onEvent: (p, {sequence}) => … });
   *   events.subscribe('contract:bid', handlers, { source: { peer }, filter: { bidderId: 'b1' } });
   */
  subscribe<K extends EventClass>(
    eventClass: K,
    handlers: EventHandlers<K>,
    opts?: SubscribeOptions<K>,
  ): Subscription {
    const topic = buildEventTopic(eventClass, opts?.source);
    const filterJson = opts?.filter ? JSON.stringify(opts.filter) : undefined;
    return this.transport.subscribe(
      topic,
      {
        onEvent: (json, sequence) => handlers.onEvent(JSON.parse(json) as EventMap[K], { sequence }),
        onError: handlers.onError,
        onClosed: handlers.onClosed,
      },
      filterJson,
    );
  }

  /**
   * EMIT a typed event. The redone `AircEventPublisher` fans it out to matching
   * subscribers (cross-grid, server-side filtered).
   */
  async emit<K extends EventClass>(eventClass: K, payload: EventMap[K]): Promise<void> {
    const body = stampContext(payload, this.contextId);
    await this.transport.emit(eventClass, JSON.stringify(body));
  }
}
