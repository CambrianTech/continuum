/**
 * Continuum Core IPC Client - TypeScript <-> Rust via Unix Socket
 *
 * Event-driven architecture:
 * - Socket.on('data') wakes when response ready (no polling)
 * - Async/await for request/response
 * - Performance timing on every call
 */

import net from 'net';
import { EventEmitter } from 'events';

// Import generated types from Rust (single source of truth)
import type {
	InboxMessageRequest,
	CognitionDecision,
	PriorityScore,
	PersonaState,
} from '../../../shared/generated';

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

interface Response {
	success: boolean;
	result?: any;
	error?: string;
}

// ============================================================================
// IPC Client
// ============================================================================

export class RustCoreIPCClient extends EventEmitter {
	private socket: net.Socket | null = null;
	private buffer = '';
	private pendingRequests: Map<number, (response: Response) => void> = new Map();
	private nextRequestId = 1;
	private connected = false;

	constructor(private socketPath: string) {
		super();
	}

	/**
	 * Connect to continuum-core server
	 */
	async connect(): Promise<void> {
		if (this.connected) {
			return;
		}

		return new Promise((resolve, reject) => {
			this.socket = net.createConnection(this.socketPath);

			this.socket.on('connect', () => {
				this.connected = true;
				this.emit('connect');
				resolve();
			});

			this.socket.on('data', (data) => {
				this.buffer += data.toString();

				// Process complete lines
				const lines = this.buffer.split('\n');
				this.buffer = lines.pop() || '';

				for (const line of lines) {
					if (line.trim()) {
						this.handleResponse(line);
					}
				}
			});

			this.socket.on('error', (err) => {
				this.emit('error', err);
				reject(err);
			});

			this.socket.on('close', () => {
				this.connected = false;
				this.emit('close');
			});
		});
	}

	private handleResponse(line: string): void {
		try {
			const response: Response & { requestId?: number } = JSON.parse(line);
			if (response.requestId !== undefined) {
				const callback = this.pendingRequests.get(response.requestId);
				if (callback) {
					callback(response);
					this.pendingRequests.delete(response.requestId);
				}
			}
		} catch (e) {
			console.error('Failed to parse response:', e, line);
		}
	}

	/**
	 * Send a request and wait for response (event-driven, no polling)
	 * Supports concurrent requests via request IDs
	 */
	private async request(command: any): Promise<Response> {
		if (!this.connected || !this.socket) {
			throw new Error('Not connected to continuum-core server');
		}

		const requestId = this.nextRequestId++;
		const requestWithId = { ...command, requestId };

		return new Promise((resolve, reject) => {
			const json = JSON.stringify(requestWithId) + '\n';
			const start = performance.now();

			this.pendingRequests.set(requestId, (response) => {
				const duration = performance.now() - start;
				if (duration > 10) {
					console.warn(`⚠️  Slow IPC call: ${command.command} took ${duration.toFixed(2)}ms`);
				}
				resolve(response);
			});

			this.socket!.write(json, (err) => {
				if (err) {
					this.pendingRequests.delete(requestId);
					reject(err);
				}
			});
		});
	}

	/**
	 * Health check
	 */
	async healthCheck(): Promise<boolean> {
		const response = await this.request({ command: 'health-check' });
		return response.success && response.result?.healthy === true;
	}

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
	 * Returns array of AI participant IDs who should receive the utterance
	 */
	async voiceOnUtterance(event: UtteranceEvent): Promise<string[]> {
		const { VOICE_RESPONSE_FIELDS } = await import('./IPCFieldNames');

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
	 * Check if TTS should be routed to a session
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

		return response.result?.should_route === true;
	}

	/**
	 * Synthesize text to speech (currently returns hold music)
	 */
	async voiceSynthesize(text: string, voice?: string, adapter?: string): Promise<{
		audio: Buffer;
		sampleRate: number;
		durationMs: number;
		adapter: string;
	}> {
		const response = await this.request({
			command: 'voice/synthesize',
			text,
			voice,
			adapter,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to synthesize speech');
		}

		// Convert base64 audio to Buffer
		const audioBase64 = response.result?.audio || '';
		const audio = Buffer.from(audioBase64, 'base64');

		return {
			audio,
			sampleRate: response.result?.sample_rate || 16000,
			durationMs: response.result?.duration_ms || 0,
			adapter: response.result?.adapter || 'unknown',
		};
	}

	/**
	 * Create a persona inbox
	 */
	async inboxCreate(personaId: string): Promise<void> {
		const response = await this.request({
			command: 'inbox/create',
			persona_id: personaId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to create inbox');
		}
	}

	// ========================================================================
	// Cognition Engine Methods
	// ========================================================================

	/**
	 * Create a cognition engine for a persona (call once on persona init)
	 */
	async cognitionCreateEngine(personaId: string, personaName: string): Promise<void> {
		const response = await this.request({
			command: 'cognition/create-engine',
			persona_id: personaId,
			persona_name: personaName,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to create cognition engine');
		}
	}

	/**
	 * Calculate message priority (sub-1ms in Rust)
	 */
	async cognitionCalculatePriority(
		personaId: string,
		content: string,
		senderType: 'human' | 'persona' | 'agent' | 'system',
		isVoice: boolean,
		roomId: string,
		timestamp: number
	): Promise<PriorityScore> {
		const response = await this.request({
			command: 'cognition/calculate-priority',
			persona_id: personaId,
			content,
			sender_type: senderType,
			is_voice: isVoice,
			room_id: roomId,
			timestamp,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to calculate priority');
		}

		return response.result as PriorityScore;
	}

	/**
	 * Fast-path decision: should we respond? (sub-1ms in Rust)
	 * Handles deduplication, mention detection, state-based gating
	 */
	async cognitionFastPathDecision(
		personaId: string,
		message: InboxMessageRequest
	): Promise<CognitionDecision> {
		const response = await this.request({
			command: 'cognition/fast-path-decision',
			persona_id: personaId,
			message,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to get fast-path decision');
		}

		return response.result as CognitionDecision;
	}

	/**
	 * Enqueue message to persona's priority inbox
	 */
	async cognitionEnqueueMessage(
		personaId: string,
		message: InboxMessageRequest
	): Promise<void> {
		const response = await this.request({
			command: 'cognition/enqueue-message',
			persona_id: personaId,
			message,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to enqueue message');
		}
	}

	/**
	 * Get persona's current state (energy, mood, attention)
	 */
	async cognitionGetState(personaId: string): Promise<PersonaState & { service_cadence_ms: number }> {
		const response = await this.request({
			command: 'cognition/get-state',
			persona_id: personaId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to get persona state');
		}

		return response.result as PersonaState & { service_cadence_ms: number };
	}

	/**
	 * Disconnect from server
	 */
	disconnect(): void {
		if (this.socket) {
			this.socket.end();
			this.socket = null;
			this.connected = false;
		}
	}
}
