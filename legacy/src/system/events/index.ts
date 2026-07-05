/**
 * System Events - Central Event System
 */

export { SYSTEM_EVENTS, type SystemEventData, type SystemEventName } from './shared/SystemEvents';
export { EventManager, type EventsInterface } from './shared/JTAGEventSystem';

// L1-1: Event-class declaration registry (Rust-truth, TS-cached).
// See docs/grid/GRID-MIGRATION-ROADMAP.md, GRID-BUS-ARCHITECTURE §2.2.
export {
	declareEventClass,
	getEventClass,
	peekEventClassCache,
	listEventClasses,
	resolveEventChannel,
	_resetEventClassCacheForTests,
	type EventClassConfig,
	type EventClassChannelStrategy,
	type EventClassUnknownSchemaPolicy,
	type ResolvedEventClassConfig,
} from './shared/EventClass';