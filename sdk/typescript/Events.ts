/**
 * Events — the typed Event primitive (EMIT + SUBSCRIBE) for the TS SDK.
 *
 * `subscribe<K>(class, handlers)` infers the payload type from the class literal
 * via the generated `EventMap`; `emit<K>(class, payload)` publishes a typed event.
 * Events = emit + subscribe (the pub/sub twin of Commands = execute + provide).
 *
 * Zero logic ([[headless-core-many-clients]]): thin typed wrapper over the facade
 * `Transport`. `EventMap` is GENERATED. Cross-grid pub/sub rides the redone
 * `AircEventPublisher`. See docs/architecture/SDK-API-SURFACE.md.
 */

import type { Transport, Subscription } from './transport';
import type { EventMap, EventClass } from './generated/CommandMap';

/** Typed event handlers — payload is the generated type for the class. */
export interface EventHandlers<K extends EventClass> {
  onEvent(event: EventMap[K]): void;
  onError?(message: string): void;
  onClosed?(): void;
}

export class Events {
  constructor(private readonly transport: Transport) {}

  /**
   * SUBSCRIBE to an event class. The class literal infers the payload type:
   * `subscribe('data:chat_messages:created', { onEvent: m => … })` types `m`.
   */
  subscribe<K extends EventClass>(eventClass: K, handlers: EventHandlers<K>): Subscription {
    return this.transport.subscribe(eventClass, {
      onEvent: (json) => handlers.onEvent(JSON.parse(json) as EventMap[K]),
      onError: handlers.onError,
      onClosed: handlers.onClosed,
    });
  }

  /**
   * EMIT a typed event. `emit('data:chat_messages:created', message)` publishes;
   * the redone `AircEventPublisher` fans it out to subscribers (cross-grid).
   */
  async emit<K extends EventClass>(eventClass: K, payload: EventMap[K]): Promise<void> {
    await this.transport.emit(eventClass, JSON.stringify(payload));
  }
}
