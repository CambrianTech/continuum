/**
 * RustCoreIPC Voice Module - Voice/TTS/STT methods
 */

import type { RustCoreIPCClientBase } from './base';

// ============================================================================
// Types
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

// ============================================================================
// Mixin
// ============================================================================

export interface VoiceSynthesizeResult {
	audio: Buffer;
	sampleRate: number;
	durationMs: number;
	numSamples: number;
	adapter: string;
}

export interface VoiceMixin {
	voiceRegisterSession(sessionId: string, roomId: string, participants: VoiceParticipant[]): Promise<void>;
	voiceOnUtterance(event: UtteranceEvent): Promise<string[]>;
	voiceShouldRouteTts(sessionId: string, personaId: string): Promise<boolean>;
	voiceSynthesize(text: string, voice?: string, adapter?: string): Promise<VoiceSynthesizeResult>;
	voiceSpeakInCall(callId: string, userId: string, text: string, voice?: string, adapter?: string): Promise<VoiceSynthesizeResult>;
	voiceInjectAudio(callId: string, userId: string, samples: number[]): Promise<void>;
}

export function VoiceMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements VoiceMixin {
		/**
		 * Register a voice session
		 */
		async voiceRegisterSession(
			sessionId: string,
			roomId: string,
			participants: VoiceParticipant[]
		): Promise<void> {
			const response = await this.request({
				command: 'voice/register-session',
				session_id: sessionId,
				room_id: roomId,
				participants,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to register session');
			}
		}

		/**
		 * Process an utterance and broadcast to ALL AI participants
		 */
		async voiceOnUtterance(event: UtteranceEvent): Promise<string[]> {
			const { VOICE_RESPONSE_FIELDS } = await import('../IPCFieldNames');

			const response = await this.request({
				command: 'voice/on-utterance',
				event,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to process utterance');
			}

			return response.result?.[VOICE_RESPONSE_FIELDS.RESPONDER_IDS] || [];
		}

		/**
		 * Check if TTS should route to a specific persona
		 */
		async voiceShouldRouteTts(sessionId: string, personaId: string): Promise<boolean> {
			const response = await this.request({
				command: 'voice/should-route-tts',
				session_id: sessionId,
				persona_id: personaId,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to check TTS routing');
			}

			return response.result?.should_route ?? false;
		}

		/**
		 * Synthesize speech from text
		 */
		async voiceSynthesize(
			text: string,
			voice?: string,
			adapter?: string
		): Promise<VoiceSynthesizeResult> {
			const { response, binaryData } = await this.requestFull({
				command: 'voice/synthesize',
				text,
				voice,
				adapter,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to synthesize speech');
			}

			// Binary framing: raw PCM bytes arrive as binaryData (no base64 decode needed)
			const audio = binaryData || Buffer.alloc(0);

			return {
				audio,
				sampleRate: response.result?.sample_rate || 16000,
				durationMs: response.result?.duration_ms || 0,
				numSamples: response.result?.num_samples || 0,
				adapter: response.result?.adapter || 'unknown',
			};
		}

		/**
		 * Inject pre-synthesized audio into a call's mixer.
		 * Used by AudioNativeBridge to push audio-native model output into the room.
		 */
		async voiceInjectAudio(
			callId: string,
			userId: string,
			samples: number[]
		): Promise<void> {
			const response = await this.request({
				command: 'voice/inject-audio',
				call_id: callId,
				user_id: userId,
				samples,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to inject audio');
			}
		}

		/**
		 * Synthesize and queue speech for a call
		 */
		async voiceSpeakInCall(
			callId: string,
			userId: string,
			text: string,
			voice?: string,
			adapter?: string
		): Promise<VoiceSynthesizeResult> {
			const { response, binaryData } = await this.requestFull({
				command: 'voice/speak-in-call',
				call_id: callId,
				user_id: userId,
				text,
				voice,
				adapter,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to speak in call');
			}

			// Binary framing: raw PCM bytes arrive as binaryData (no base64 decode needed)
			const audio = binaryData || Buffer.alloc(0);

			return {
				audio,
				sampleRate: response.result?.sample_rate || 16000,
				durationMs: response.result?.duration_ms || 0,
				numSamples: response.result?.num_samples || 0,
				adapter: response.result?.adapter || 'unknown',
			};
		}
	};
}
