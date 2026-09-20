/**
 * VoiceOrchestratorRustBridge - Delegates to Rust VoiceOrchestrator via IPC
 *
 * Broadcasts utterances to ALL text-based AI participants.
 * No turn-taking. No gating. No cooldowns.
 * Rust handles the fast path (filtering, participant lookup).
 */

import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../core/continuum-core/bindings/RustCoreIPC';
import type { UtteranceEvent } from './VoiceOrchestrator';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { UserEntity } from '../../data/entities/UserEntity';
import { DataList } from '../../../commands/data/list/shared/DataListTypes';
import { getAIAudioBridge } from './AIAudioBridge';

/**
 * Rust-backed VoiceOrchestrator — broadcasts to all, no gating
 */
export class VoiceOrchestratorRustBridge {
	private static _instance: VoiceOrchestratorRustBridge | null = null;
	private client: RustCoreIPCClient;
	private connected = false;
	// #193 slice B: the call IS its airc room, so every call-scoped wire verb is
	// keyed by roomId — the server's canonical call id (slice A, PR #2095). The
	// browser-tab sessionId is a WHO/WHERE-from axis, not a call identity; verbs
	// that only receive it (onUtterance, endSession) translate through this map.
	// The server's legacy-alias layer would also resolve untranslated ids, but a
	// client that sends the canonical id is the cutover — the server's MIRROR
	// ROOM probe going silent is slice B's done-signal.
	private callRoomBySession = new Map<string, string>();

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
		} catch {
			// Connection to Rust core failed — will retry on next operation
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
				dbHandle: 'default',
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
		// The call is registered under roomId on BOTH wire fields (#193 slice B):
		// session_id == room_id means the LiveKit call IS the airc room, no
		// parallel identity. sessionId is remembered only to translate later
		// verbs that still carry it.
		this.callRoomBySession.set(sessionId, roomId);
		await this.client.voiceRegisterSession(roomId, roomId, rustParticipants);

		// Register AI participants with AIAudioBridge so speak() works when
		// persona:response:generated fires. Without this, isInCall() returns false
		// and AI responses are silently dropped. Keyed by roomId — the same
		// canonical call id the Rust side serves speak-in-call under.
		const bridge = getAIAudioBridge();
		for (const p of rustParticipants) {
			if (p.participant_type !== 'human') {
				const user = userMap.get(p.user_id);
				await bridge.joinCall(roomId, p.user_id as UUID, user?.displayName || p.display_name);
			}
		}

	}

	/**
	 * Process utterance — returns ALL text-based AI participant IDs (broadcast model)
	 */
	async onUtterance(event: UtteranceEvent): Promise<UUID[]> {
		if (!this.connected) {
			return [];
		}

		const rustEvent = {
			// Translate the tab session to the canonical call id (== roomId,
			// #193 slice B); an unknown session passes through untranslated and
			// the server's legacy-alias layer resolves it.
			session_id: this.callRoomBySession.get(event.sessionId) ?? event.sessionId,
			speaker_id: event.speakerId,
			speaker_name: event.speakerName,
			speaker_type: event.speakerType,
			transcript: event.transcript,
			confidence: event.confidence,
			timestamp: event.timestamp,
		};

		const responderIds = await this.client.voiceOnUtterance(rustEvent);
		return responderIds as UUID[];
	}

	/**
	 * End a voice session — tells Rust to drop all agents, listeners, and state for this call.
	 * Without this, LiveKitAgent instances and Room connections leak indefinitely.
	 */
	async endSession(sessionId: UUID): Promise<void> {
		if (!this.connected) {
			return;
		}

		const callId = this.callRoomBySession.get(sessionId) ?? sessionId;
		this.callRoomBySession.delete(sessionId);
		try {
			await this.client.voiceEndSession(callId);
		} catch (err) {
			console.error(`[VoiceOrchestratorRustBridge] Failed to end session ${callId}:`, err);
		}
	}
}

export function getRustVoiceOrchestrator(): VoiceOrchestratorRustBridge {
	return VoiceOrchestratorRustBridge.instance;
}
