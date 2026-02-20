/**
 * TypeScript FFI bindings for continuum-core (Rust)
 *
 * Loads the Rust dylib via Node.js ffi-napi and provides type-safe wrappers.
 *
 * Architecture:
 * - Event-driven (no polling, sleep until FFI call)
 * - Zero-copy where possible (ArrayBuffer transfers)
 * - Performance timing on every FFI call
 */

import ffi from 'ffi-napi';
import ref from 'ref-napi';
import path from 'path';
import { performance } from 'perf_hooks';

// ============================================================================
// C Types
// ============================================================================

const voidPtr = ref.refType(ref.types.void);
const charPtr = ref.refType(ref.types.char);

// ============================================================================
// Load Rust dylib
// ============================================================================

const libraryPath = path.join(
	__dirname,
	'../../target/release',
	process.platform === 'darwin' ? 'libcontinuum_core.dylib' :
	process.platform === 'win32' ? 'continuum_core.dll' :
	'libcontinuum_core.so'
);

const lib = ffi.Library(libraryPath, {
	// Initialization
	continuum_init: ['int', [charPtr]],
	continuum_health_check: ['int', []],
	continuum_get_stats: [charPtr, [charPtr]],
	continuum_free_string: ['void', [charPtr]],

	// VoiceOrchestrator
	continuum_voice_create: [voidPtr, []],
	continuum_voice_free: ['void', [voidPtr]],
	continuum_voice_register_session: ['int', [voidPtr, charPtr, charPtr, charPtr]],
	continuum_voice_on_utterance: ['int', [voidPtr, charPtr, charPtr]],
	// PersonaInbox
	continuum_inbox_create: [voidPtr, [charPtr]],
	continuum_inbox_free: ['void', [voidPtr]],

	// Memory management
	continuum_free: ['void', [voidPtr]],
});

// ============================================================================
// Performance Timing Wrapper
// ============================================================================

interface TimingStats {
	operation: string;
	durationMs: number;
	timestamp: number;
}

const timingHistory: TimingStats[] = [];
const MAX_TIMING_HISTORY = 1000;

function timeFfiCall<T>(operation: string, fn: () => T): T {
	const start = performance.now();
	try {
		return fn();
	} finally {
		const durationMs = performance.now() - start;

		// Log slow FFI calls
		if (durationMs > 10) {
			console.warn(`⚠️  Slow FFI call: ${operation} took ${durationMs.toFixed(2)}ms`);
		}

		// Record timing
		timingHistory.push({
			operation,
			durationMs,
			timestamp: Date.now(),
		});

		// Keep history bounded
		if (timingHistory.length > MAX_TIMING_HISTORY) {
			timingHistory.shift();
		}
	}
}

// ============================================================================
// RustCore - Main API
// ============================================================================

export class RustCore {
	private static initialized = false;

	/**
	 * Initialize continuum-core with logger socket path
	 */
	static init(loggerSocketPath: string): void {
		if (this.initialized) {
			return;
		}

		const result = timeFfiCall('continuum_init', () =>
			lib.continuum_init(Buffer.from(loggerSocketPath + '\0', 'utf-8'))
		);

		if (result !== 0) {
			throw new Error('Failed to initialize continuum-core');
		}

		this.initialized = true;
		console.log('✅ Continuum core initialized');
	}

	/**
	 * Health check - verifies FFI is working
	 */
	static healthCheck(): boolean {
		return timeFfiCall('health_check', () => lib.continuum_health_check() === 1);
	}

	/**
	 * Get performance statistics from Rust core
	 */
	static getStats(category?: string): any {
		const categoryPtr = category
			? Buffer.from(category + '\0', 'utf-8')
			: ref.NULL;

		const jsonPtr = timeFfiCall('get_stats', () =>
			lib.continuum_get_stats(categoryPtr)
		);

		if (jsonPtr.isNull()) {
			return null;
		}

		const jsonStr = ref.readCString(jsonPtr, 0);
		lib.continuum_free_string(jsonPtr);

		return JSON.parse(jsonStr);
	}

	/**
	 * Get TypeScript-side FFI timing statistics
	 */
	static getFfiTimingStats() {
		if (timingHistory.length === 0) {
			return null;
		}

		const durations = timingHistory.map(t => t.durationMs);
		durations.sort((a, b) => a - b);

		return {
			count: durations.length,
			min: durations[0],
			max: durations[durations.length - 1],
			mean: durations.reduce((a, b) => a + b, 0) / durations.length,
			p50: durations[Math.floor(durations.length * 0.5)],
			p95: durations[Math.floor(durations.length * 0.95)],
			p99: durations[Math.floor(durations.length * 0.99)],
			recent: timingHistory.slice(-10),
		};
	}
}

// ============================================================================
// VoiceOrchestrator - Rust-backed turn arbitration
// ============================================================================

export interface VoiceParticipant {
	user_id: string;
	display_name: string;
	participant_type: 'human' | 'persona' | 'agent';
	expertise: string[];
}

export interface UtteranceEvent {
	session_id: string;
	speaker_id: string;
	speaker_name: string;
	speaker_type: 'human' | 'persona' | 'agent';
	transcript: string;
	confidence: number;
	timestamp: number;
}

export class VoiceOrchestrator {
	private ptr: Buffer;

	constructor() {
		this.ptr = timeFfiCall('voice_create', () => lib.continuum_voice_create());

		if (this.ptr.isNull()) {
			throw new Error('Failed to create VoiceOrchestrator');
		}
	}

	/**
	 * Register a voice session with participants
	 */
	registerSession(
		sessionId: string,
		roomId: string,
		participants: VoiceParticipant[]
	): void {
		const sessionIdBuf = Buffer.from(sessionId + '\0', 'utf-8');
		const roomIdBuf = Buffer.from(roomId + '\0', 'utf-8');
		const participantsJson = Buffer.from(JSON.stringify(participants) + '\0', 'utf-8');

		const result = timeFfiCall('voice_register_session', () =>
			lib.continuum_voice_register_session(
				this.ptr,
				sessionIdBuf,
				roomIdBuf,
				participantsJson
			)
		);

		if (result !== 0) {
			throw new Error('Failed to register voice session');
		}
	}

	/**
	 * Process an utterance and get selected responder (if any)
	 */
	onUtterance(event: UtteranceEvent): string | null {
		const eventJson = Buffer.from(JSON.stringify(event) + '\0', 'utf-8');
		const responderIdBuf = Buffer.alloc(37); // UUID + null terminator

		const result = timeFfiCall('voice_on_utterance', () =>
			lib.continuum_voice_on_utterance(this.ptr, eventJson, responderIdBuf)
		);

		if (result === 0) {
			// Responder selected
			return ref.readCString(responderIdBuf, 0);
		} else if (result === 1) {
			// No responder (statement, not question)
			return null;
		} else {
			throw new Error('Failed to process utterance');
		}
	}

	/**
	 * Free Rust resources
	 */
	destroy(): void {
		if (!this.ptr.isNull()) {
			timeFfiCall('voice_free', () => lib.continuum_voice_free(this.ptr));
			this.ptr = ref.NULL_POINTER;
		}
	}
}

// ============================================================================
// PersonaInbox - Rust-backed priority queue
// ============================================================================

export class PersonaInbox {
	private ptr: Buffer;

	constructor(personaId: string) {
		const personaIdBuf = Buffer.from(personaId + '\0', 'utf-8');

		this.ptr = timeFfiCall('inbox_create', () =>
			lib.continuum_inbox_create(personaIdBuf)
		);

		if (this.ptr.isNull()) {
			throw new Error('Failed to create PersonaInbox');
		}
	}

	/**
	 * Free Rust resources
	 */
	destroy(): void {
		if (!this.ptr.isNull()) {
			timeFfiCall('inbox_free', () => lib.continuum_inbox_free(this.ptr));
			this.ptr = ref.NULL_POINTER;
		}
	}
}
