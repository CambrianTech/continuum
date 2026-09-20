/**
 * Handle — an addressable resource (a URI) for long-running / streaming / stateful
 * work. The establish-once-reuse-many pattern (substrate `InferenceHandleStore`,
 * AI-COMMAND-NAMESPACE.md §2), made addressable: an `open`-style command returns a
 * handle carrying its `airc://` URI (in the result, or an airc `Location`-style
 * header), and EVERYTHING routes to that URI:
 *   - further commands  → `<uri>/<subcommand>`  (write/read/…; wherever the
 *     resource lives — a peer, an :vr env, N hops away)
 *   - event stream      → `<uri>/events/<class>` (fed by ANY link in a router
 *     chain; the live stream from across the grid)
 *   - close             → `<uri>/close`
 *
 * So commands + events + handles + routing collapse into one addressing scheme.
 * Zero logic ([[headless-core-many-clients]]): thin over the facade Transport.
 * See docs/architecture/SDK-API-SURFACE.md § "Handles are addressable resources".
 */

import type { Transport, Subscription, RawEventHandlers } from './transport';

/** Typed handler for a handle's event stream (payload typing is per-resource;
 *  generated handle maps refine this — kept `unknown` here at the generic base). */
export interface HandleEventHandlers {
  onEvent: (event: unknown, sequence: number) => void;
  onError?: (message: string) => void;
  onClosed?: () => void;
}

export class Handle {
  /** The resource's airc:// URI — the routing key for every op + its event stream. */
  constructor(readonly uri: string, private readonly transport: Transport) {}

  /** Run a handle-scoped subcommand, routed to the handle's URI (write/read/…).
   *  `params` is a JSON object body; per-resource typing is refined by the
   *  generated handle maps over this generic base. */
  async execute<R>(subcommand: string, params: Record<string, unknown>): Promise<R> {
    const resultJson = await this.transport.execute(
      `${this.uri}/${subcommand}`,
      JSON.stringify(params),
    );
    return JSON.parse(resultJson) as R;
  }

  /** Subscribe to the handle's event stream — fed by any link across the grid. */
  on(eventClass: string, handlers: HandleEventHandlers): Subscription {
    const raw: RawEventHandlers = {
      onEvent: (json, sequence) => { handlers.onEvent(JSON.parse(json), sequence); },
      onError: handlers.onError,
      onClosed: handlers.onClosed,
    };
    return this.transport.subscribe(`${this.uri}/events/${eventClass}`, raw);
  }

  /** Release the resource. */
  async close(): Promise<void> {
    await this.transport.execute(`${this.uri}/close`, '{}');
  }
}

/** Wrap an open-style command's result (which carries the resource URI) into a
 *  Handle. The URI comes from the result body or an airc Location-style header —
 *  the caller passes whichever the facade surfaced. */
export function handleFrom(uri: string, transport: Transport): Handle {
  return new Handle(uri, transport);
}
