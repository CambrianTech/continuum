/**
 * EventClass — TS thin-SDK unit tests.
 *
 * Validates the cache behavior + the wire-shape integration with the Rust
 * registry via a mock IPC client (so this test doesn't require the Rust
 * binary to be running).
 *
 * Roadmap item L1-1 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
 *
 * Suites are split into multiple top-level `describe` blocks (one per
 * public function) to stay under the max-lines-per-function lint limit.
 * Common per-test mock reset lives in `resetMocks` below.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import type { ResolvedEventClassConfig } from '@shared/generated/events';

// Mock the RustCoreIPC module BEFORE importing EventClass.
// EventClass dynamic-imports the IPC client, so the mock has to be in
// place by the time the dynamic import resolves.
const mockEventsDeclareClass = vi.fn();
const mockEventsGetClass = vi.fn();
const mockEventsListClasses = vi.fn();
const mockEventsResolveChannel = vi.fn();

vi.mock('../../../../core/continuum-core/bindings/RustCoreIPC', () => {
	const mockClient = {
		eventsDeclareClass: mockEventsDeclareClass,
		eventsGetClass: mockEventsGetClass,
		eventsListClasses: mockEventsListClasses,
		eventsResolveChannel: mockEventsResolveChannel,
	};
	return {
		RustCoreIPCClient: {
			getInstanceAsync: vi.fn(() => Promise.resolve(mockClient)),
		},
	};
});

import {
	declareEventClass,
	getEventClass,
	peekEventClassCache,
	listEventClasses,
	resolveEventChannel,
	_resetEventClassCacheForTests,
} from '@system/events/shared/EventClass';

function makeResolved(name: string, broadcast = false, channel: 'local' | 'global' = 'local'): ResolvedEventClassConfig {
	return {
		name,
		broadcast,
		channel,
		schemaVersion: 'v1',
		onUnknownSchema: 'fail',
		description: '',
	};
}

// Per-suite reset — extracted so each top-level describe stays under the
// max-lines-per-function lint limit while keeping a clean fixture.
function resetMocks(): void {
	_resetEventClassCacheForTests();
	mockEventsDeclareClass.mockReset();
	mockEventsGetClass.mockReset();
	mockEventsListClasses.mockReset();
	mockEventsResolveChannel.mockReset();
}

describe('EventClass — declareEventClass', () => {
	beforeEach(resetMocks);

	it('forwards to Rust IPC + primes the cache', async () => {
		const resolved = makeResolved('test:local-class');
		mockEventsDeclareClass.mockResolvedValueOnce(resolved);

		const result = await declareEventClass('test:local-class', {
			broadcast: false,
			schemaVersion: 'v1',
		});

		expect(result).toEqual(resolved);
		expect(mockEventsDeclareClass).toHaveBeenCalledWith({
			name: 'test:local-class',
			broadcast: false,
			schemaVersion: 'v1',
		});
		// Cache primed — peek hits without another IPC call.
		expect(peekEventClassCache('test:local-class')).toEqual(resolved);
	});

	it('propagates wire-contract errors (conflicting redeclare)', async () => {
		mockEventsDeclareClass.mockRejectedValueOnce(new Error('conflicting redeclaration'));
		await expect(
			declareEventClass('test:conflict', { broadcast: false, schemaVersion: 'v1' }),
		).rejects.toThrow(/conflicting redeclaration/);
	});
});

describe('EventClass — getEventClass (read-through cache)', () => {
	beforeEach(resetMocks);

	it('caches a successful lookup so the second call skips IPC', async () => {
		const resolved = makeResolved('test:cached');
		mockEventsGetClass.mockResolvedValueOnce(resolved);

		const first = await getEventClass('test:cached');
		const second = await getEventClass('test:cached');

		expect(first).toEqual(resolved);
		expect(second).toEqual(resolved);
		expect(mockEventsGetClass).toHaveBeenCalledTimes(1);
	});

	it('caches the null (undeclared) case', async () => {
		mockEventsGetClass.mockResolvedValueOnce(null);

		const first = await getEventClass('test:never-declared');
		const second = await getEventClass('test:never-declared');

		expect(first).toBeNull();
		expect(second).toBeNull();
		// Undeclared MUST also be cached — otherwise the hot path would
		// keep paying IPC for events whose class will never be declared.
		expect(mockEventsGetClass).toHaveBeenCalledTimes(1);
	});

	it('dedups in-flight concurrent lookups', async () => {
		const resolved = makeResolved('test:concurrent');
		// Resolve the IPC promise on the next tick so two callers race.
		mockEventsGetClass.mockImplementationOnce(
			() => new Promise(resolve => setTimeout(() => resolve(resolved), 5)),
		);

		const [a, b] = await Promise.all([
			getEventClass('test:concurrent'),
			getEventClass('test:concurrent'),
		]);

		expect(a).toEqual(resolved);
		expect(b).toEqual(resolved);
		// Both callers share ONE IPC round-trip.
		expect(mockEventsGetClass).toHaveBeenCalledTimes(1);
	});
});

describe('EventClass — peekEventClassCache (sync hot path)', () => {
	beforeEach(resetMocks);

	it('returns undefined when never looked up', () => {
		expect(peekEventClassCache('test:cold')).toBeUndefined();
	});

	it('returns the cached resolved config after declare', async () => {
		const resolved = makeResolved('test:warm');
		mockEventsDeclareClass.mockResolvedValueOnce(resolved);

		await declareEventClass('test:warm', { broadcast: false, schemaVersion: 'v1' });

		// Sync — no await on peek. This is the property the hot
		// emit path relies on.
		expect(peekEventClassCache('test:warm')).toEqual(resolved);
	});

	it('returns null when the cached lookup was undeclared', async () => {
		mockEventsGetClass.mockResolvedValueOnce(null);

		await getEventClass('test:undecl-warm');

		expect(peekEventClassCache('test:undecl-warm')).toBeNull();
	});
});

describe('EventClass — listEventClasses', () => {
	beforeEach(resetMocks);

	it('returns all classes + warms the cache for each', async () => {
		const a = makeResolved('test:list-a');
		const b = makeResolved('test:list-b', true, 'global');
		mockEventsListClasses.mockResolvedValueOnce([a, b]);

		const list = await listEventClasses();

		expect(list).toEqual([a, b]);
		// After list, both classes are warm — emit hot path no longer
		// pays IPC for them.
		expect(peekEventClassCache('test:list-a')).toEqual(a);
		expect(peekEventClassCache('test:list-b')).toEqual(b);
	});
});

describe('EventClass — resolveEventChannel', () => {
	beforeEach(resetMocks);

	it('forwards to Rust IPC and returns the channel string', async () => {
		mockEventsResolveChannel.mockResolvedValueOnce('global');

		const channel = await resolveEventChannel('test:resolve-global', { foo: 'bar' });

		expect(channel).toBe('global');
		expect(mockEventsResolveChannel).toHaveBeenCalledWith('test:resolve-global', { foo: 'bar' });
	});

	it('propagates IPC errors (e.g. ByRoomId missing payload field)', async () => {
		mockEventsResolveChannel.mockRejectedValueOnce(
			new Error("event class 'chat:posted' requires field 'roomId' in payload"),
		);

		await expect(
			resolveEventChannel('chat:posted', {}),
		).rejects.toThrow(/requires field 'roomId'/);
	});
});
