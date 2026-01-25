/**
 * VoiceOrchestratorRustBridge - Swaps TypeScript VoiceOrchestrator with Rust implementation
 *
 * This is the "wildly different integration" test:
 * - TypeScript VoiceWebSocketHandler continues to work unchanged
 * - But underneath, it calls Rust continuum-core via IPC
 * - If this works seamlessly, the API is proven correct
 *
 * Performance target: <1ms overhead vs TypeScript implementation
 */

import { RustCoreIPCClient } from '../../../workers/continuum-core/bindings/RustCoreIPC';
import type { UtteranceEvent } from './VoiceOrchestrator';
import type { UUID } from '../../core/types/CrossPlatformUUID';

interface VoiceParticipant {
	userId: UUID;
	displayName: string;
	type: 'human' | 'persona' | 'agent';
	expertise?: string[];
}

/**
 * Rust-backed VoiceOrchestrator
 *
 * Drop-in replacement for TypeScript VoiceOrchestrator.
 * Uses continuum-core via IPC (0.13ms latency measured).
 */
export class VoiceOrchestratorRustBridge {
	private static _instance: VoiceOrchestratorRustBridge | null = null;
	private client: RustCoreIPCClient;
	private connected = false;

	// Session state (mirrors TypeScript implementation)
	private sessionParticipants: Map<UUID, VoiceParticipant[]> = new Map();

	// TTS callback (set by VoiceWebSocketHandler)
	private ttsCallback: ((sessionId: UUID, personaId: UUID, text: string) => Promise<void>) | null = null;

	private constructor() {
		this.client = new RustCoreIPCClient('/tmp/continuum-core.sock');
		this.initializeConnection();
	}

	static get instance(): VoiceOrchestratorRustBridge {
		if (!VoiceOrchestratorRustBridge._instance) {
			VoiceOrchestratorRustBridge._instance = new VoiceOrchestratorRustBridge();
		}
		return VoiceOrchestratorRustBridge._instance;
	}

	private async initializeConnection(): Promise<void> {
		try {
			await this.client.connect();
			this.connected = true;
			console.log('🦀 VoiceOrchestrator: Connected to Rust core');
		} catch (e) {
			console.error('❌ VoiceOrchestrator: Failed to connect to Rust core:', e);
			console.error('   Falling back to TypeScript implementation would go here');
		}
	}

	/**
	 * Set the TTS callback for routing voice responses
	 */
	setTTSCallback(callback: (sessionId: UUID, personaId: UUID, text: string) => Promise<void>): void {
		this.ttsCallback = callback;
	}

	/**
	 * Register participants for a voice session
	 *
	 * Delegates to Rust VoiceOrchestrator via IPC
	 */
	async registerSession(sessionId: UUID, roomId: UUID, participants: VoiceParticipant[]): Promise<void> {
		if (!this.connected) {
			await this.initializeConnection();
		}

		// Store participants locally (needed for TTS routing)
		this.sessionParticipants.set(sessionId, participants);

		// Convert to Rust format
		const rustParticipants = participants.map(p => ({
			user_id: p.userId,
			display_name: p.displayName,
			participant_type: p.type,
			expertise: p.expertise || [],
		}));

		// Call Rust VoiceOrchestrator via IPC
		try {
			await this.client.voiceRegisterSession(sessionId, roomId, rustParticipants);
			console.log(`🦀 VoiceOrchestrator: Registered session ${sessionId} with ${participants.length} participants`);
		} catch (e) {
			console.error('❌ VoiceOrchestrator: Failed to register session:', e);
			throw e;
		}
	}

	/**
	 * Process an utterance and broadcast to ALL AI participants
	 * Returns array of AI participant IDs who should receive the utterance
	 *
	 * This is the critical path - must be <1ms overhead
	 */
	async onUtterance(event: UtteranceEvent): Promise<UUID[]> {
		if (!this.connected) {
			console.warn('⚠️  VoiceOrchestrator: Not connected to Rust core, skipping');
			return [];
		}

		const start = performance.now();

		try {
			// Convert to Rust format
			const rustEvent = {
				session_id: event.sessionId,
				speaker_id: event.speakerId,
				speaker_name: event.speakerName,
				speaker_type: event.speakerType,
				transcript: event.transcript,
				confidence: event.confidence,
				timestamp: event.timestamp,
			};

			// Call Rust VoiceOrchestrator via IPC - returns ALL AI participant IDs
			const responderIds = await this.client.voiceOnUtterance(rustEvent);

			const duration = performance.now() - start;

			if (duration > 5) {
				console.warn(`⚠️  VoiceOrchestrator: Slow utterance processing: ${duration.toFixed(2)}ms`);
			} else {
				console.log(`🦀 VoiceOrchestrator: Processed utterance in ${duration.toFixed(2)}ms → ${responderIds.length} AI participants`);
			}

			return responderIds as UUID[];
		} catch (e) {
			console.error('❌ VoiceOrchestrator: Failed to process utterance:', e);
			return [];
		}
	}

	/**
	 * Check if TTS should be routed to a specific session
	 *
	 * Called when a persona responds to determine if it should go to voice
	 */
	async shouldRouteToTTS(sessionId: UUID, personaId: UUID): Promise<boolean> {
		if (!this.connected) {
			return false;
		}

		try {
			return await this.client.voiceShouldRouteTts(sessionId, personaId);
		} catch (e) {
			console.error('❌ VoiceOrchestrator: Failed to check TTS routing:', e);
			return false;
		}
	}

	/**
	 * Route a text response to TTS
	 *
	 * Called when a persona responds and should use voice output
	 */
	async routeToTTS(sessionId: UUID, personaId: UUID, text: string): Promise<void> {
		if (!this.ttsCallback) {
			console.warn('⚠️  VoiceOrchestrator: No TTS callback set');
			return;
		}

		try {
			await this.ttsCallback(sessionId, personaId, text);
		} catch (e) {
			console.error('❌ VoiceOrchestrator: Failed to route to TTS:', e);
		}
	}

	/**
	 * End a voice session
	 */
	async endSession(sessionId: UUID): Promise<void> {
		this.sessionParticipants.delete(sessionId);
		console.log(`🦀 VoiceOrchestrator: Ended session ${sessionId}`);
	}
}

/**
 * Get the Rust-backed VoiceOrchestrator instance
 */
export function getRustVoiceOrchestrator(): VoiceOrchestratorRustBridge {
	return VoiceOrchestratorRustBridge.instance;
}
