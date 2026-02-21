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
	is_audio_native: boolean;
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

export interface SttAdapterInfo {
	name: string;
	initialized: boolean;
	description: string;
}

export interface SttListResult {
	adapters: SttAdapterInfo[];
	active: string;
}

export interface TranscribeResult {
	text: string;
	language: string;
	confidence: number;
	adapter: string;
	segments: Array<{ text: string; start_ms: number; end_ms: number }>;
}

export interface TestAudioGenerateResult {
	audio: string;  // base64-encoded i16 LE PCM
	samples: number;
	duration_ms: number;
	noise_type: string;
	sample_rate: number;
}

export interface TranscriptionEntry {
	call_id: string;
	speaker_id: string;
	speaker_name: string;
	text: string;
	timestamp_ms: number;
}

export interface PollTranscriptionsResult {
	transcriptions: TranscriptionEntry[];
	count: number;
}

export interface VoiceMixin {
	voiceRegisterSession(sessionId: string, roomId: string, participants: VoiceParticipant[]): Promise<void>;
	voiceOnUtterance(event: UtteranceEvent): Promise<string[]>;
	voiceSynthesize(text: string, voice?: string, adapter?: string): Promise<VoiceSynthesizeResult>;
	voiceSpeakInCall(callId: string, userId: string, text: string, voice?: string, adapter?: string): Promise<VoiceSynthesizeResult>;
	voiceInjectAudio(callId: string, userId: string, samples: number[]): Promise<void>;
	voiceAmbientAdd(callId: string, sourceName: string): Promise<{ handle: string; source_name: string }>;
	voiceAmbientInject(callId: string, handle: string, samples: number[]): Promise<void>;
	voiceAmbientRemove(callId: string, handle: string): Promise<void>;
	voiceSttList(): Promise<SttListResult>;
	voiceTranscribeWithAdapter(audio: string, adapter: string, language?: string): Promise<TranscribeResult>;
	voiceTestAudioGenerate(noiseType: string, durationMs: number, params?: Record<string, any>): Promise<TestAudioGenerateResult>;
	voicePollTranscriptions(callId?: string): Promise<PollTranscriptionsResult>;
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

		/**
		 * Add an ambient audio source to a call (TV, music, background noise).
		 * Returns a handle for injecting audio and removing the source later.
		 */
		async voiceAmbientAdd(
			callId: string,
			sourceName: string,
		): Promise<{ handle: string; source_name: string }> {
			const response = await this.request({
				command: 'voice/ambient-add',
				call_id: callId,
				source_name: sourceName,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to add ambient source');
			}

			return response.result as { handle: string; source_name: string };
		}

		/**
		 * Inject audio into an ambient source by handle.
		 */
		async voiceAmbientInject(
			callId: string,
			handle: string,
			samples: number[],
		): Promise<void> {
			const response = await this.request({
				command: 'voice/ambient-inject',
				call_id: callId,
				handle,
				samples,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to inject ambient audio');
			}
		}

		/**
		 * Remove an ambient audio source from a call.
		 */
		async voiceAmbientRemove(
			callId: string,
			handle: string,
		): Promise<void> {
			const response = await this.request({
				command: 'voice/ambient-remove',
				call_id: callId,
				handle,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to remove ambient source');
			}
		}

		/**
		 * List all registered STT adapters and the currently active one.
		 */
		async voiceSttList(): Promise<SttListResult> {
			const response = await this.request({
				command: 'voice/stt-list',
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to list STT adapters');
			}

			return response.result as SttListResult;
		}

		/**
		 * Transcribe audio using a specific STT adapter without changing the global active adapter.
		 * @param audio - base64-encoded i16 LE PCM at 16kHz
		 * @param adapter - adapter name (e.g., "whisper", "moonshine")
		 * @param language - optional language code (e.g., "en")
		 */
		async voiceTranscribeWithAdapter(
			audio: string,
			adapter: string,
			language?: string,
		): Promise<TranscribeResult> {
			const response = await this.request({
				command: 'voice/transcribe-with-adapter',
				audio,
				adapter,
				language,
			});

			if (!response.success) {
				throw new Error(response.error || `Failed to transcribe with adapter '${adapter}'`);
			}

			return response.result as TranscribeResult;
		}

		/**
		 * Generate test audio noise via the Rust TestAudioGenerator.
		 * Returns base64-encoded i16 LE PCM at 16kHz.
		 * @param noiseType - noise type name (crowd, factory, gunfire, explosion, siren, music, wind, rain, tv_dialogue)
		 * @param durationMs - duration in milliseconds
		 * @param params - optional parameters (e.g., { voice_count: 5 } for crowd, { shots_per_second: 3 } for gunfire)
		 */
		async voiceTestAudioGenerate(
			noiseType: string,
			durationMs: number,
			params?: Record<string, any>,
		): Promise<TestAudioGenerateResult> {
			const response = await this.request({
				command: 'voice/test-audio-generate',
				noise_type: noiseType,
				duration_ms: durationMs,
				params,
			});

			if (!response.success) {
				throw new Error(response.error || `Failed to generate test audio: ${noiseType}`);
			}

			return response.result as TestAudioGenerateResult;
		}

		/**
		 * Poll and drain transcriptions from the STT listener buffer.
		 * Returns all transcriptions since the last poll, optionally filtered by call_id.
		 * Used by integration tests to verify E2E audio roundtrip.
		 */
		async voicePollTranscriptions(callId?: string): Promise<PollTranscriptionsResult> {
			const response = await this.request({
				command: 'voice/poll-transcriptions',
				...(callId ? { call_id: callId } : {}),
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to poll transcriptions');
			}

			return response.result as PollTranscriptionsResult;
		}
	};
}
