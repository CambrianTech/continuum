/**
 * Continuum Core IPC Client - TypeScript <-> Rust via Unix Socket
 *
 * Binary framing protocol (length-prefixed):
 * - Requests: Newline-delimited JSON (TypeScript → Rust)
 * - Responses: [4 bytes u32 BE length][payload] (Rust → TypeScript)
 *   - JSON-only: payload = JSON bytes
 *   - Binary: payload = JSON bytes + \0 + raw binary bytes
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
	ActivityDomain,
	ChannelRegistryStatus,
	ChannelEnqueueRequest,
	ServiceCycleResult,
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

/** Full IPC response including optional binary payload */
interface IPCResponse {
	response: Response;
	binaryData?: Buffer;
}

// ============================================================================
// IPC Client
// ============================================================================

export class RustCoreIPCClient extends EventEmitter {
	private socket: net.Socket | null = null;
	/** Binary buffer for length-prefixed frame parsing */
	private _buffer: Buffer = Buffer.alloc(0);
	private pendingRequests: Map<number, (result: IPCResponse) => void> = new Map();
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

			this.socket.on('data', (data: Buffer) => {
				this.onData(data);
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

	/**
	 * Process incoming binary data using length-prefixed framing.
	 * Frame format: [4 bytes u32 BE length][payload]
	 * - JSON-only: payload = UTF-8 JSON
	 * - Binary: payload = UTF-8 JSON + \0 separator + raw binary
	 */
	private onData(data: Buffer): void {
		this._buffer = Buffer.concat([this._buffer, data]);

		// Process all complete frames in buffer
		while (this._buffer.length >= 4) {
			const totalLength = this._buffer.readUInt32BE(0);
			const frameEnd = 4 + totalLength;

			if (this._buffer.length < frameEnd) {
				break; // Need more data
			}

			// Extract complete frame payload
			const payload = this._buffer.subarray(4, frameEnd);
			this._buffer = this._buffer.subarray(frameEnd);

			// Find \0 separator — unambiguous because serde_json encodes
			// null chars as \u0000 (never raw 0x00 in JSON output)
			const separatorIndex = payload.indexOf(0);

			let jsonBytes: Buffer;
			let binaryData: Buffer | undefined;

			if (separatorIndex !== -1) {
				// Binary frame: [JSON][\0][raw binary bytes]
				jsonBytes = payload.subarray(0, separatorIndex);
				binaryData = payload.subarray(separatorIndex + 1);
			} else {
				// JSON-only frame
				jsonBytes = payload;
			}

			// Parse JSON response
			try {
				const response: Response & { requestId?: number } = JSON.parse(jsonBytes.toString('utf8'));
				this.handleResponse(response, binaryData);
			} catch (e) {
				console.error('Failed to parse IPC response JSON:', e);
			}
		}
	}

	private handleResponse(response: Response & { requestId?: number }, binaryData?: Buffer): void {
		if (response.requestId !== undefined) {
			const callback = this.pendingRequests.get(response.requestId);
			if (callback) {
				callback({ response, binaryData });
				this.pendingRequests.delete(response.requestId);
			}
		}
	}

	/**
	 * Send a request and wait for full response (including optional binary data).
	 * Used internally by request() and by methods that need binary payloads.
	 */
	private async requestFull(command: any): Promise<IPCResponse> {
		if (!this.connected || !this.socket) {
			throw new Error('Not connected to continuum-core server');
		}

		const requestId = this.nextRequestId++;
		const requestWithId = { ...command, requestId };

		return new Promise((resolve, reject) => {
			const json = JSON.stringify(requestWithId) + '\n';
			const start = performance.now();

			this.pendingRequests.set(requestId, (result) => {
				const duration = performance.now() - start;
				if (duration > 10) {
					console.warn(`⚠️  Slow IPC call: ${command.command} took ${duration.toFixed(2)}ms`);
				}
				resolve(result);
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
	 * Send a request and wait for JSON response (ignores binary payload).
	 * Used by most IPC methods that don't need raw binary data.
	 */
	private async request(command: any): Promise<Response> {
		const { response } = await this.requestFull(command);
		return response;
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
	 * Synthesize text to speech and return raw PCM audio via binary IPC framing.
	 * Returns raw i16 LE PCM bytes — no base64 encoding overhead.
	 *
	 * NOTE: For live voice calls, prefer voiceSpeakInCall() which injects audio
	 * directly into the Rust call mixer without audio ever leaving the Rust process.
	 */
	async voiceSynthesize(text: string, voice?: string, adapter?: string): Promise<{
		audio: Buffer;
		sampleRate: number;
		durationMs: number;
		numSamples: number;
		adapter: string;
	}> {
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
	 * Synthesize text and inject audio directly into a live call's mixer.
	 * Audio NEVER leaves the Rust process — this method only returns metadata.
	 *
	 * Flow: TypeScript sends text → Rust synthesizes → injects into call mixer →
	 * audio streams to browsers via existing WebSocket binary frames.
	 *
	 * @param callId - The call to inject audio into
	 * @param userId - The participant (persona) who is "speaking"
	 * @param text - The text to synthesize
	 * @param voice - Optional voice name
	 * @param adapter - Optional TTS adapter (e.g., "kokoro")
	 */
	async voiceSpeakInCall(callId: string, userId: string, text: string, voice?: string, adapter?: string): Promise<{
		numSamples: number;
		durationMs: number;
		sampleRate: number;
	}> {
		const response = await this.request({
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

		return {
			numSamples: response.result?.num_samples || 0,
			durationMs: response.result?.duration_ms || 0,
			sampleRate: response.result?.sample_rate || 16000,
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

	// ========================================================================
	// Channel System Methods
	// ========================================================================

	/**
	 * Enqueue an item into the channel system.
	 * Item is routed to the correct domain channel (AUDIO/CHAT/BACKGROUND).
	 */
	async channelEnqueue(
		personaId: string,
		item: ChannelEnqueueRequest
	): Promise<{ routed_to: ActivityDomain; status: ChannelRegistryStatus }> {
		const response = await this.request({
			command: 'channel/enqueue',
			persona_id: personaId,
			item,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to enqueue channel item');
		}

		return response.result as { routed_to: ActivityDomain; status: ChannelRegistryStatus };
	}

	/**
	 * Dequeue highest-priority item from a specific domain or any domain.
	 */
	async channelDequeue(
		personaId: string,
		domain?: ActivityDomain
	): Promise<{ item: any | null; has_more: boolean }> {
		const response = await this.request({
			command: 'channel/dequeue',
			persona_id: personaId,
			domain: domain ?? null,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to dequeue channel item');
		}

		return response.result as { item: any | null; has_more: boolean };
	}

	/**
	 * Get per-channel status snapshot.
	 */
	async channelStatus(personaId: string): Promise<ChannelRegistryStatus> {
		const response = await this.request({
			command: 'channel/status',
			persona_id: personaId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to get channel status');
		}

		return response.result as ChannelRegistryStatus;
	}

	/**
	 * Run one service cycle: consolidate all channels, return next item to process.
	 * This is the main scheduling entry point — replaces TS-side channel iteration.
	 */
	async channelServiceCycle(personaId: string): Promise<{
		should_process: boolean;
		item: any | null;
		channel: ActivityDomain | null;
		wait_ms: number;
		stats: ChannelRegistryStatus;
	}> {
		const response = await this.request({
			command: 'channel/service-cycle',
			persona_id: personaId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to run service cycle');
		}

		// Convert bigint wait_ms to number (Rust u64 → ts-rs bigint → JS number)
		const result = response.result;
		return {
			should_process: result.should_process,
			item: result.item ?? null,
			channel: result.channel ?? null,
			wait_ms: Number(result.wait_ms),
			stats: result.stats,
		};
	}

	/**
	 * Clear all channel queues for a persona.
	 */
	async channelClear(personaId: string): Promise<void> {
		const response = await this.request({
			command: 'channel/clear',
			persona_id: personaId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to clear channels');
		}
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
