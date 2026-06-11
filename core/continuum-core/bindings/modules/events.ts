/**
 * RustCoreIPC Events Module — event-class declaration registry.
 *
 * Roadmap item L1-1 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
 * Spec: GRID-BUS-ARCHITECTURE §2.2 (continuum#1439).
 *
 * The Rust crate `events::` is the canonical store. This mixin is the
 * thin SDK wrapper — the TS thin shim at src/system/events/shared/
 * EventClass.ts caches reads locally for the hot emit-path but only
 * mutates through here.
 *
 * Native-truth-thin-SDK-per-language: the names + meanings of fields
 * are owned by Rust; ts-rs generates the wire types under
 * `shared/generated/events/`. Methods on this mixin are just typed
 * IPC wrappers — no business logic.
 */

import type { RustCoreIPCClientBase } from './base';
import type {
	EventClassConfig,
	ResolvedEventClassConfig,
} from '../../../../protocol/typescript/events';

// ============================================================================
// IPC params + result shapes
// ============================================================================

/**
 * Params for `events/declare-class` — the class name + flattened
 * `EventClassConfig` (broadcast / channel / schemaVersion / etc.).
 *
 * The Rust handler uses `#[serde(flatten)]` so the config fields live
 * at the top level of the request alongside `name`.
 */
export interface EventsDeclareClassParams extends EventClassConfig {
	name: string;
}

export interface EventsResolveChannelResult {
	channel: string;
}

// ============================================================================
// Mixin
// ============================================================================

export interface EventsMixin {
	/**
	 * Register a new event class. Idempotent for identical re-declarations;
	 * throws on conflicting re-declarations (wire-contract integrity —
	 * silently shifting transport behavior between callers would mask bugs).
	 *
	 * Returns the canonical, post-validation form (with all defaults filled).
	 */
	eventsDeclareClass(params: EventsDeclareClassParams): Promise<ResolvedEventClassConfig>;

	/**
	 * Look up a single class's resolved config. Returns `null` when
	 * undeclared — callers fall back to default backward-compat behavior
	 * (local + WebSocket only, no airc broadcast).
	 */
	eventsGetClass(name: string): Promise<ResolvedEventClassConfig | null>;

	/**
	 * Snapshot of all declared classes. Used by the TS-side cache on
	 * startup + by `grid/show-event-classes` introspection.
	 */
	eventsListClasses(): Promise<ResolvedEventClassConfig[]>;

	/**
	 * Resolve the airc channel for an emit. Used by the L1-2
	 * AircEventTransport when it lands. Throws if the class isn't
	 * declared, isn't `broadcast: true`, or its payload-dependent
	 * channel strategy can't find the required field
	 * (e.g. ByRoomId without `roomId` in payload).
	 */
	eventsResolveChannel(name: string, payload: Record<string, unknown>): Promise<string>;
}

// Mixin generic constraint mirrors the pattern in sibling mixins
// (GpuMixin, CognitionMixin, DatasetMixin). `any[]` is the only constructor
// signature TypeScript's mixin pattern accepts — `unknown[]` would reject
// subclass constructors with concrete arg types.
/* eslint-disable @typescript-eslint/no-explicit-any */
export function EventsMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(
	Base: T,
): T & (new (...args: any[]) => EventsMixin) {
	return class extends Base implements EventsMixin {
		async eventsDeclareClass(params: EventsDeclareClassParams): Promise<ResolvedEventClassConfig> {
			const response = await this.request({
				command: 'events/declare-class',
				...params,
			});
			if (!response.success) {
				throw new Error(response.error ?? `events/declare-class failed for '${params.name}'`);
			}
			return response.result as ResolvedEventClassConfig;
		}

		async eventsGetClass(name: string): Promise<ResolvedEventClassConfig | null> {
			const response = await this.request({ command: 'events/get-class', name });
			if (!response.success) {
				throw new Error(response.error ?? `events/get-class failed for '${name}'`);
			}
			// Rust returns JSON null when undeclared — surface as TS null,
			// not undefined, so callers can distinguish "not declared" from
			// "didn't ask yet."
			return (response.result as ResolvedEventClassConfig | null) ?? null;
		}

		async eventsListClasses(): Promise<ResolvedEventClassConfig[]> {
			const response = await this.request({ command: 'events/list-classes' });
			if (!response.success) {
				throw new Error(response.error ?? 'events/list-classes failed');
			}
			return response.result as ResolvedEventClassConfig[];
		}

		async eventsResolveChannel(name: string, payload: Record<string, unknown>): Promise<string> {
			const response = await this.request({
				command: 'events/resolve-channel',
				name,
				payload,
			});
			if (!response.success) {
				throw new Error(response.error ?? `events/resolve-channel failed for '${name}'`);
			}
			return (response.result as EventsResolveChannelResult).channel;
		}
	};
}
/* eslint-enable @typescript-eslint/no-explicit-any */
