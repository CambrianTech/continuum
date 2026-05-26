/**
 * EventClass — thin TS shim over the Rust event-class registry.
 *
 * Roadmap item L1-1 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
 * Spec: GRID-BUS-ARCHITECTURE §2.2 (continuum#1439).
 *
 * Native-truth-thin-SDK-per-language: declarations are stored canonically
 * in Rust (`crate::events::event_class_registry`). This module is the
 * thin TS wrapper:
 *
 *   1. Re-exports the generated wire types (single source of truth).
 *   2. Provides `declareEventClass(name, config)` — typed wrapper that
 *      calls the Rust `events/declare-class` IPC via `RustCoreIPCClient`.
 *   3. Provides `getEventClass(name)` — read-through cache for the hot
 *      `Events.emit()` path. First lookup hits the registry once via IPC,
 *      result is cached for the lifetime of the process. Declarations
 *      are immutable once made (conflicting re-declare throws on the
 *      Rust side), so cache-invalidation isn't needed.
 *   4. Provides `resolveEventChannel(name, payload)` — the airc transport
 *      consults this at emit time. Channel resolution is payload-dependent
 *      (ByRoomId / ByPeerId), so this can't be precomputed — but the
 *      class config it reads from IS cached.
 *
 * Why local cache: `Events.emit()` is in the hot path. A round-trip to
 * Rust on every emit would add ~1ms per event. With a local read-through
 * cache, only the first lookup pays IPC; everything after is a Map.get.
 *
 * What the cache does NOT do: it does not mutate. All declarations go
 * through the IPC. Two processes that both call `declareEventClass`
 * with conflicting configs will get one success + one error from the
 * Rust registry — the cache cannot mask this.
 *
 * Mutability semantics: declarations are append-only. Once a class is
 * declared in Rust, identical re-declarations succeed (idempotent);
 * conflicting re-declarations throw. The cache therefore never has to
 * invalidate — what it has is final.
 *
 * Why this bypasses `Commands.execute()`: the registry is a foundational
 * primitive — declared event classes are what `Events.emit()` consults
 * to know whether/where to broadcast. Going through Commands.execute()
 * here would create a layering inversion (the bus would consult event
 * metadata that requires the bus to fetch). Direct IPC keeps the
 * dependency one-way. The CLI/introspection surface (`grid/show-event-classes`)
 * can be added as a separate TS Command when needed (L4 roadmap item).
 */

// Use a dynamic import to dodge the shared/server divide — this module
// lives in `shared/` but the RustCoreIPCClient is server-only. Browser
// callers shouldn't be declaring event classes (they consume the bus,
// they don't shape it), but they may import the *types* from here.
import type {
	EventClassConfig,
	EventClassChannelStrategy,
	EventClassUnknownSchemaPolicy,
	ResolvedEventClassConfig,
} from '@shared/generated/events';

// Re-export the generated wire types so callers can import them from
// `@system/events/shared/EventClass` (a stable path) without reaching
// into `@shared/generated/events` directly.
export type {
	EventClassConfig,
	EventClassChannelStrategy,
	EventClassUnknownSchemaPolicy,
	ResolvedEventClassConfig,
};

// ─── IPC client access (server-only, lazy-loaded) ───────────────────────

interface RustIPCClient {
	eventsDeclareClass(params: EventClassConfig & { name: string }): Promise<ResolvedEventClassConfig>;
	eventsGetClass(name: string): Promise<ResolvedEventClassConfig | null>;
	eventsListClasses(): Promise<ResolvedEventClassConfig[]>;
	eventsResolveChannel(name: string, payload: Record<string, unknown>): Promise<string>;
}

let cachedClientPromise: Promise<RustIPCClient> | null = null;

async function getRustClient(): Promise<RustIPCClient> {
	if (cachedClientPromise) return cachedClientPromise;
	cachedClientPromise = (async (): Promise<RustIPCClient> => {
		// Dynamic import so this module stays loadable in browser bundles
		// (where the import would fail). Browser consumers should only
		// import types from here, never call the imperative functions.
		const mod = await import('../../../workers/continuum-core/bindings/RustCoreIPC');
		const client = await mod.RustCoreIPCClient.getInstanceAsync();
		return client as unknown as RustIPCClient;
	})();
	return cachedClientPromise;
}

// ─── Read-through cache ─────────────────────────────────────────────────

/**
 * Process-local cache of resolved event-class configs. Keyed by class name.
 *
 * Three states represented:
 *   - Missing key      — never looked up.
 *   - `null` value     — looked up; Rust said "not declared".
 *   - `ResolvedEventClassConfig` — looked up; declared.
 *
 * The `null` case is cached separately so a hot-path emit on an undeclared
 * class doesn't keep paying IPC.
 */
const classCache = new Map<string, ResolvedEventClassConfig | null>();

/**
 * In-flight dedup — if two callers ask for the same class concurrently
 * before the first IPC returns, they share one round-trip.
 */
const inFlight = new Map<string, Promise<ResolvedEventClassConfig | null>>();

/**
 * Test-only: clear the local cache. Production code does not need this —
 * declarations are append-only and the cache never goes stale. Used by
 * unit tests that exercise the IPC path repeatedly with different state.
 */
export function _resetEventClassCacheForTests(): void {
	classCache.clear();
	inFlight.clear();
	cachedClientPromise = null;
}

// ─── Public API ─────────────────────────────────────────────────────────

/**
 * Register an event class. Idempotent for identical re-declarations;
 * throws on conflicting re-declarations (wire-contract integrity).
 *
 * Most callers declare their classes once at module-load time:
 *
 *   await declareEventClass('presence:peer-manifest', {
 *     broadcast: true,
 *     channel: 'global',
 *     schemaVersion: 'v1',
 *     description: 'Peer-manifest advertisements (BGP-style route ads)',
 *   });
 */
export async function declareEventClass(
	name: string,
	config: EventClassConfig,
): Promise<ResolvedEventClassConfig> {
	const client = await getRustClient();
	const resolved = await client.eventsDeclareClass({ name, ...config });
	// Prime the cache with the canonical form so the very next emit
	// doesn't have to round-trip back.
	classCache.set(name, resolved);
	return resolved;
}

/**
 * Look up a class's resolved config, with local read-through caching.
 *
 * Returns `null` when the class is undeclared — callers fall back to
 * default backward-compat behavior (local + WebSocket only, no airc).
 * The `null` result is itself cached so undeclared classes don't keep
 * paying IPC on the hot path.
 */
export async function getEventClass(name: string): Promise<ResolvedEventClassConfig | null> {
	if (classCache.has(name)) {
		return classCache.get(name) ?? null;
	}
	const pending = inFlight.get(name);
	if (pending) return pending;

	const lookup = (async (): Promise<ResolvedEventClassConfig | null> => {
		try {
			const client = await getRustClient();
			const result = await client.eventsGetClass(name);
			classCache.set(name, result ?? null);
			return result ?? null;
		} finally {
			inFlight.delete(name);
		}
	})();
	inFlight.set(name, lookup);
	return lookup;
}

/**
 * Synchronous cache peek. Returns:
 *   - `ResolvedEventClassConfig` if cached + declared
 *   - `null` if cached + undeclared
 *   - `undefined` if not yet looked up
 *
 * Useful for the hot emit-path: if the class is already cached, emit can
 * make a sync decision; if not, emit either falls back to default
 * behavior or kicks off an async lookup. Whichever is right for the
 * caller's latency budget.
 */
export function peekEventClassCache(name: string): ResolvedEventClassConfig | null | undefined {
	return classCache.get(name);
}

/**
 * Snapshot of all declared classes — fresh from the registry, NOT from
 * the local cache. Used by introspection commands (`grid/show-event-classes`)
 * and by startup paths that prime the cache.
 *
 * Side effect: populates the cache with every class returned, so
 * subsequent `peekEventClassCache` / `getEventClass` calls hit local
 * memory.
 */
export async function listEventClasses(): Promise<ResolvedEventClassConfig[]> {
	const client = await getRustClient();
	const list = await client.eventsListClasses();
	for (const cls of list) {
		classCache.set(cls.name, cls);
	}
	return list;
}

/**
 * Resolve the airc channel an emit of `name` should land on.
 *
 * Throws if:
 *   - The class isn't declared.
 *   - The class is `broadcast: false` (no channel to resolve).
 *   - The class's channel strategy is payload-dependent and the payload
 *     doesn't carry the required field (e.g. ByRoomId without `roomId`).
 *
 * The L1-2 AircEventTransport consults this at emit time to decide
 * which gist / channel to write the event to.
 */
export async function resolveEventChannel(
	name: string,
	payload: Record<string, unknown>,
): Promise<string> {
	const client = await getRustClient();
	return client.eventsResolveChannel(name, payload);
}
