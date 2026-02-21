/**
 * VoiceOrchestratorRustBridge - Delegates to Rust VoiceOrchestrator via IPC
 *
 * Broadcasts utterances to ALL text-based AI participants.
 * No turn-taking. No gating. No cooldowns.
 * Rust handles the fast path (filtering, participant lookup).
 */

import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../workers/continuum-core/bindings/RustCoreIPC';
import type { UtteranceEvent } from './VoiceOrchestrator';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { UserEntity } from '../../data/entities/UserEntity';
import { DataList } from '../../../commands/data/list/shared/DataListTypes';

/**
 * Rust-backed VoiceOrchestrator — broadcasts to all, no gating
 */
export class VoiceOrchestratorRustBridge {
	private static _instance: VoiceOrchestratorRustBridge | null = null;
	private client: RustCoreIPCClient;
	private connected = false;

	private constructor() {
		this.client = new RustCoreIPCClient(getContinuumCoreSocketPath());
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
			console.error('🦀 VoiceOrchestrator: Failed to connect to Rust core:', e);
		}
	}

	/**
	 * Register participants for a voice session via Rust IPC.
	 * Looks up user types from database to correctly classify human vs persona vs agent.
	 * Rust orchestrator uses participant_type to route transcriptions to AI responders.
	 */
	async registerSession(sessionId: UUID, roomId: UUID, participantIds: UUID[]): Promise<void> {
		if (!this.connected) {
			await this.initializeConnection();
		}

		// Look up actual user types from database — MUST distinguish human from AI
		const userMap = new Map<string, UserEntity>();
		if (participantIds.length > 0) {
			const result = await DataList.execute<UserEntity>({
				collection: 'users',
				filter: { id: { $in: participantIds } },
				limit: participantIds.length,
			});
			if (result.success && result.items) {
				for (const user of result.items) {
					userMap.set(user.id, user);
				}
			}
		}

		const rustParticipants = participantIds.map(id => {
			const user = userMap.get(id);
			// Map UserType to Rust SpeakerType: 'persona'/'agent' → AI, 'human'/'system' → human
			let participantType: 'human' | 'persona' | 'agent' = 'human';
			if (user?.type === 'persona') participantType = 'persona';
			else if (user?.type === 'agent') participantType = 'agent';
			return {
				user_id: id,
				display_name: user?.displayName || '',
				participant_type: participantType,
				expertise: [] as string[],
				is_audio_native: false,
			};
		});

		const aiCount = rustParticipants.filter(p => p.participant_type !== 'human').length;
		await this.client.voiceRegisterSession(sessionId, roomId, rustParticipants);
		console.log(`🦀 VoiceOrchestrator: Registered session ${sessionId.slice(0, 8)} with ${participantIds.length} participants (${aiCount} AI)`);
	}

	/**
	 * Process utterance — returns ALL text-based AI participant IDs (broadcast model)
	 */
	async onUtterance(event: UtteranceEvent): Promise<UUID[]> {
		if (!this.connected) {
			console.error('🦀 VoiceOrchestrator: Not connected to Rust core');
			return [];
		}

		const start = performance.now();

		const rustEvent = {
			session_id: event.sessionId,
			speaker_id: event.speakerId,
			speaker_name: event.speakerName,
			speaker_type: event.speakerType,
			transcript: event.transcript,
			confidence: event.confidence,
			timestamp: event.timestamp,
		};

		const responderIds = await this.client.voiceOnUtterance(rustEvent);
		const duration = performance.now() - start;

		if (duration > 5) {
			console.warn(`🦀 VoiceOrchestrator: Slow utterance: ${duration.toFixed(2)}ms`);
		}

		return responderIds as UUID[];
	}

	/**
	 * End a voice session
	 */
	async endSession(sessionId: UUID): Promise<void> {
		console.log(`🦀 VoiceOrchestrator: Ended session ${sessionId.slice(0, 8)}`);
	}
}

export function getRustVoiceOrchestrator(): VoiceOrchestratorRustBridge {
	return VoiceOrchestratorRustBridge.instance;
}
