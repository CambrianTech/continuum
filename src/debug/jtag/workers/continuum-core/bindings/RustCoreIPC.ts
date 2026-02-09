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
import path from 'path';
import { EventEmitter } from 'events';
import { SOCKETS } from '../../../shared/config';

/**
 * Resolve socket path to absolute path.
 * Socket config uses relative paths from project root.
 * This helper resolves them to absolute paths for Unix socket connections.
 */
export function resolveSocketPath(socketPath: string): string {
	// If already absolute, return as-is
	if (path.isAbsolute(socketPath)) {
		return socketPath;
	}
	// Resolve relative to current working directory (project root)
	return path.resolve(process.cwd(), socketPath);
}

/**
 * Get the default continuum-core socket path (resolved to absolute).
 * Use this instead of hardcoding paths.
 */
export function getContinuumCoreSocketPath(): string {
	return resolveSocketPath(SOCKETS.CONTINUUM_CORE);
}

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
	// Code module types
	EditMode,
	ReadResult,
	WriteResult,
	SearchMatch,
	SearchResult,
	TreeNode,
	TreeResult,
	UndoResult,
	ChangeNode,
	HistoryResult,
	GitStatusInfo,
	// Shell session types
	ShellExecuteResponse,
	ShellPollResponse,
	ShellSessionInfo,
	ShellWatchResponse,
	SentinelRule,
} from '../../../shared/generated';

// Memory subsystem types (Hippocampus in Rust — corpus-based, no SQL)
import type { CorpusMemory } from './CorpusMemory';
import type { CorpusTimelineEvent } from './CorpusTimelineEvent';
import type { LoadCorpusResponse } from './LoadCorpusResponse';
import type { MemoryRecallResponse } from './MemoryRecallResponse';
import type { MultiLayerRecallRequest } from './MultiLayerRecallRequest';
import type { ConsciousnessContextResponse } from './ConsciousnessContextResponse';

// RAG module types (batched parallel loading via Rayon)
import type {
	RagSourceRequest,
	RagComposeResult,
} from '../../../shared/generated/rag';

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
// Code Module Types — imported from ts-rs generated (Rust is source of truth)
// All code types imported at top level from shared/generated
// ============================================================================

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

	/** Rate-limit slow IPC warnings globally: command -> last warning timestamp */
	private static slowWarningTimestamps: Map<string, number> = new Map();
	private static readonly SLOW_IPC_THRESHOLD_MS = 500;
	private static readonly SLOW_WARNING_COOLDOWN_MS = 10_000;

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
				if (duration > RustCoreIPCClient.SLOW_IPC_THRESHOLD_MS) {
					const now = Date.now();
					const lastWarned = RustCoreIPCClient.slowWarningTimestamps.get(command.command) ?? 0;
					if (now - lastWarned > RustCoreIPCClient.SLOW_WARNING_COOLDOWN_MS) {
						RustCoreIPCClient.slowWarningTimestamps.set(command.command, now);
						console.warn(`⚠️  Slow IPC call: ${command.command} took ${duration.toFixed(0)}ms`);
					}
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
	 * Service cycle + fast-path decision in ONE IPC call.
	 * Eliminates a separate round-trip for fastPathDecision.
	 * Returns service_cycle result + optional cognition decision.
	 */
	async channelServiceCycleFull(personaId: string): Promise<{
		should_process: boolean;
		item: any | null;
		channel: ActivityDomain | null;
		wait_ms: number;
		stats: ChannelRegistryStatus;
		decision: CognitionDecision | null;
	}> {
		const response = await this.request({
			command: 'channel/service-cycle-full',
			persona_id: personaId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to run full service cycle');
		}

		const result = response.result;
		return {
			should_process: result.should_process,
			item: result.item ?? null,
			channel: result.channel ?? null,
			wait_ms: Number(result.wait_ms),
			stats: result.stats,
			decision: result.decision ?? null,
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

	// ========================================================================
	// Memory Subsystem (Hippocampus in Rust — corpus-based, no SQL)
	// ========================================================================

	/**
	 * Load a persona's full memory corpus into Rust's in-memory cache.
	 * Called at persona startup — sends all memories + timeline events from TS ORM.
	 * Subsequent recall operations run on this cached corpus.
	 */
	async memoryLoadCorpus(
		personaId: string,
		memories: CorpusMemory[],
		events: CorpusTimelineEvent[]
	): Promise<LoadCorpusResponse> {
		const response = await this.request({
			command: 'memory/load-corpus',
			persona_id: personaId,
			memories,
			events,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to load memory corpus');
		}

		return response.result as LoadCorpusResponse;
	}

	/**
	 * Append a single memory to the cached corpus (incremental update).
	 * Called after Hippocampus stores a new memory to the DB.
	 * Keeps Rust cache coherent with the ORM without full reload.
	 */
	async memoryAppendMemory(
		personaId: string,
		memory: CorpusMemory
	): Promise<void> {
		const response = await this.request({
			command: 'memory/append-memory',
			persona_id: personaId,
			memory,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to append memory to corpus');
		}
	}

	/**
	 * Append a single timeline event to the cached corpus (incremental update).
	 */
	async memoryAppendEvent(
		personaId: string,
		event: CorpusTimelineEvent
	): Promise<void> {
		const response = await this.request({
			command: 'memory/append-event',
			persona_id: personaId,
			event,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to append event to corpus');
		}
	}

	/**
	 * 6-layer parallel multi-recall (the big improvement)
	 */
	async memoryMultiLayerRecall(
		personaId: string,
		params: MultiLayerRecallRequest
	): Promise<MemoryRecallResponse> {
		const response = await this.request({
			command: 'memory/multi-layer-recall',
			persona_id: personaId,
			...params,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to run multi-layer recall');
		}

		return response.result as MemoryRecallResponse;
	}

	/**
	 * Build consciousness context for RAG injection
	 * Replaces UnifiedConsciousness.getContext() in TS
	 */
	async memoryConsciousnessContext(
		personaId: string,
		roomId: string,
		currentMessage?: string,
		skipSemanticSearch?: boolean
	): Promise<ConsciousnessContextResponse> {
		const response = await this.request({
			command: 'memory/consciousness-context',
			persona_id: personaId,
			room_id: roomId,
			current_message: currentMessage ?? null,
			skip_semantic_search: skipSemanticSearch ?? false,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to build consciousness context');
		}

		return response.result as ConsciousnessContextResponse;
	}

	// ========================================================================
	// Code Module Methods (file operations, change tracking, code intelligence)
	// ========================================================================

	/**
	 * Initialize a per-persona workspace with file engine and change graph.
	 * Must be called before any other code/* operations for this persona.
	 *
	 * @param personaId - The persona's UUID
	 * @param workspaceRoot - Absolute path to the persona's workspace directory
	 * @param readRoots - Optional read-only root directories (e.g., main codebase for discovery)
	 */
	async codeCreateWorkspace(
		personaId: string,
		workspaceRoot: string,
		readRoots?: string[]
	): Promise<void> {
		const response = await this.request({
			command: 'code/create-workspace',
			persona_id: personaId,
			workspace_root: workspaceRoot,
			read_roots: readRoots ?? [],
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to create workspace');
		}
	}

	/**
	 * Read a file or line range from the persona's workspace.
	 */
	async codeRead(
		personaId: string,
		filePath: string,
		startLine?: number,
		endLine?: number
	): Promise<ReadResult> {
		const response = await this.request({
			command: 'code/read',
			persona_id: personaId,
			file_path: filePath,
			start_line: startLine ?? null,
			end_line: endLine ?? null,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to read file');
		}

		return response.result as ReadResult;
	}

	/**
	 * Write or create a file in the persona's workspace.
	 * Creates a ChangeNode in the change graph for undo support.
	 */
	async codeWrite(
		personaId: string,
		filePath: string,
		content: string,
		description?: string
	): Promise<WriteResult> {
		const response = await this.request({
			command: 'code/write',
			persona_id: personaId,
			file_path: filePath,
			content,
			description: description ?? null,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to write file');
		}

		return response.result as WriteResult;
	}

	/**
	 * Edit a file using one of four edit modes:
	 * - line_range: Replace content between line numbers
	 * - search_replace: Find and replace text
	 * - insert_at: Insert content at a specific line
	 * - append: Add content to end of file
	 */
	async codeEdit(
		personaId: string,
		filePath: string,
		editMode: EditMode,
		description?: string
	): Promise<WriteResult> {
		const response = await this.request({
			command: 'code/edit',
			persona_id: personaId,
			file_path: filePath,
			edit_mode: editMode,
			description: description ?? null,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to edit file');
		}

		return response.result as WriteResult;
	}

	/**
	 * Delete a file from the persona's workspace.
	 * Full content is preserved in the change graph for undo.
	 */
	async codeDelete(
		personaId: string,
		filePath: string,
		description?: string
	): Promise<WriteResult> {
		const response = await this.request({
			command: 'code/delete',
			persona_id: personaId,
			file_path: filePath,
			description: description ?? null,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to delete file');
		}

		return response.result as WriteResult;
	}

	/**
	 * Preview an edit as a unified diff without applying it.
	 */
	async codeDiff(
		personaId: string,
		filePath: string,
		editMode: EditMode
	): Promise<{ success: boolean; unified: string }> {
		const response = await this.request({
			command: 'code/diff',
			persona_id: personaId,
			file_path: filePath,
			edit_mode: editMode,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to compute diff');
		}

		return response.result as { success: boolean; unified: string };
	}

	/**
	 * Undo a specific change or the last N changes.
	 * Pass changeId to undo a specific operation, or count to undo last N.
	 */
	async codeUndo(
		personaId: string,
		changeId?: string,
		count?: number
	): Promise<UndoResult> {
		const response = await this.request({
			command: 'code/undo',
			persona_id: personaId,
			change_id: changeId ?? null,
			count: count ?? null,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to undo');
		}

		return response.result as UndoResult;
	}

	/**
	 * Get change history for a file or entire workspace.
	 */
	async codeHistory(
		personaId: string,
		filePath?: string,
		limit?: number
	): Promise<HistoryResult> {
		const response = await this.request({
			command: 'code/history',
			persona_id: personaId,
			file_path: filePath ?? null,
			limit: limit ?? null,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to get history');
		}

		return response.result as HistoryResult;
	}

	/**
	 * Search for a regex pattern across workspace files.
	 * Respects .gitignore, supports glob filtering.
	 */
	async codeSearch(
		personaId: string,
		pattern: string,
		fileGlob?: string,
		maxResults?: number
	): Promise<SearchResult> {
		const response = await this.request({
			command: 'code/search',
			persona_id: personaId,
			pattern,
			file_glob: fileGlob ?? null,
			max_results: maxResults ?? null,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to search');
		}

		return response.result as SearchResult;
	}

	/**
	 * Generate a directory tree for the workspace.
	 */
	async codeTree(
		personaId: string,
		path?: string,
		maxDepth?: number,
		includeHidden?: boolean
	): Promise<TreeResult> {
		const response = await this.request({
			command: 'code/tree',
			persona_id: personaId,
			path: path ?? null,
			max_depth: maxDepth ?? null,
			include_hidden: includeHidden ?? false,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to generate tree');
		}

		return response.result as TreeResult;
	}

	/**
	 * Get git status for the workspace.
	 */
	async codeGitStatus(personaId: string): Promise<GitStatusInfo> {
		const response = await this.request({
			command: 'code/git-status',
			persona_id: personaId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to get git status');
		}

		return response.result as GitStatusInfo;
	}

	/**
	 * Get git diff for the workspace.
	 */
	async codeGitDiff(personaId: string, staged?: boolean): Promise<{ success: boolean; diff: string }> {
		const response = await this.request({
			command: 'code/git-diff',
			persona_id: personaId,
			staged: staged ?? false,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to get git diff');
		}

		return response.result as { success: boolean; diff: string };
	}

	/**
	 * Get git log for the workspace.
	 */
	async codeGitLog(personaId: string, count?: number): Promise<{ success: boolean; log: string }> {
		const response = await this.request({
			command: 'code/git-log',
			persona_id: personaId,
			count: count ?? 10,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to get git log');
		}

		return response.result as { success: boolean; log: string };
	}

	/**
	 * Stage files for commit.
	 */
	async codeGitAdd(personaId: string, paths: string[]): Promise<{ staged: string[] }> {
		const response = await this.request({
			command: 'code/git-add',
			persona_id: personaId,
			paths,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to stage files');
		}

		return response.result as { staged: string[] };
	}

	/**
	 * Create a git commit.
	 */
	async codeGitCommit(personaId: string, message: string): Promise<{ hash: string }> {
		const response = await this.request({
			command: 'code/git-commit',
			persona_id: personaId,
			message,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to create commit');
		}

		return response.result as { hash: string };
	}

	/**
	 * Push to remote.
	 */
	async codeGitPush(personaId: string, remote?: string, branch?: string): Promise<{ output: string }> {
		const response = await this.request({
			command: 'code/git-push',
			persona_id: personaId,
			remote: remote ?? '',
			branch: branch ?? '',
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to push');
		}

		return response.result as { output: string };
	}

	// ── Shell Session Methods ──────────────────────────────────────

	/**
	 * Create a shell session for a workspace.
	 */
	async shellCreate(personaId: string, workspaceRoot: string): Promise<ShellSessionInfo> {
		const response = await this.request({
			command: 'code/shell-create',
			persona_id: personaId,
			workspace_root: workspaceRoot,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to create shell session');
		}

		return response.result as ShellSessionInfo;
	}

	/**
	 * Execute a command in a shell session.
	 *
	 * Two modes:
	 * - `wait: false` (default) — returns immediately with execution handle. Poll for output.
	 * - `wait: true` — blocks until completion, returns full stdout/stderr.
	 */
	async shellExecute(
		personaId: string,
		cmd: string,
		options?: { timeoutMs?: number; wait?: boolean },
	): Promise<ShellExecuteResponse> {
		const response = await this.request({
			command: 'code/shell-execute',
			persona_id: personaId,
			cmd,
			timeout_ms: options?.timeoutMs ?? null,
			wait: options?.wait ?? false,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to execute command');
		}

		return response.result as ShellExecuteResponse;
	}

	/**
	 * Poll an execution for new output since last poll.
	 * Call repeatedly until `finished` is true.
	 */
	async shellPoll(personaId: string, executionId: string): Promise<ShellPollResponse> {
		const response = await this.request({
			command: 'code/shell-poll',
			persona_id: personaId,
			execution_id: executionId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to poll execution');
		}

		return response.result as ShellPollResponse;
	}

	/**
	 * Kill a running execution.
	 */
	async shellKill(personaId: string, executionId: string): Promise<void> {
		const response = await this.request({
			command: 'code/shell-kill',
			persona_id: personaId,
			execution_id: executionId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to kill execution');
		}
	}

	/**
	 * Change shell session working directory.
	 */
	async shellCd(personaId: string, path: string): Promise<{ cwd: string }> {
		const response = await this.request({
			command: 'code/shell-cd',
			persona_id: personaId,
			path,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to change directory');
		}

		return response.result as { cwd: string };
	}

	/**
	 * Get shell session status/info.
	 */
	async shellStatus(personaId: string): Promise<ShellSessionInfo> {
		const response = await this.request({
			command: 'code/shell-status',
			persona_id: personaId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to get shell status');
		}

		return response.result as ShellSessionInfo;
	}

	/**
	 * Destroy a shell session (kills all running executions).
	 */
	async shellDestroy(personaId: string): Promise<void> {
		const response = await this.request({
			command: 'code/shell-destroy',
			persona_id: personaId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to destroy shell session');
		}
	}

	/**
	 * Watch a shell execution for new output.
	 * Blocks until output is available — no timeout, no polling.
	 * Returns classified output lines filtered through sentinel rules.
	 */
	async shellWatch(personaId: string, executionId: string): Promise<ShellWatchResponse> {
		const response = await this.request({
			command: 'code/shell-watch',
			persona_id: personaId,
			execution_id: executionId,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to watch execution');
		}

		return response.result as ShellWatchResponse;
	}

	/**
	 * Configure sentinel filter rules on a shell execution.
	 * Rules classify output lines and control which are emitted or suppressed during watch.
	 */
	async shellSentinel(personaId: string, executionId: string, rules: SentinelRule[]): Promise<{ applied: boolean; ruleCount: number }> {
		const response = await this.request({
			command: 'code/shell-sentinel',
			persona_id: personaId,
			execution_id: executionId,
			rules,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to set sentinel rules');
		}

		return response.result as { applied: boolean; ruleCount: number };
	}

	// ========================================================================
	// Model Discovery Methods
	// ========================================================================

	/**
	 * Discover model metadata from provider APIs.
	 * ALL HTTP I/O runs in Rust (off Node.js main thread).
	 * Returns discovered models for ModelRegistry population.
	 */
	async modelsDiscover(providers: Array<{
		provider_id: string;
		api_key: string;
		base_url: string;
		static_models?: Array<{
			id: string;
			context_window: number;
			max_output_tokens?: number;
			capabilities?: string[];
			cost_per_1k_tokens?: { input: number; output: number };
		}>;
	}>): Promise<{
		models: Array<{
			modelId: string;
			contextWindow: number;
			maxOutputTokens?: number;
			provider: string;
			capabilities?: string[];
			costPer1kTokens?: { input: number; output: number };
			discoveredAt: number;
		}>;
		count: number;
		providers: number;
	}> {
		const response = await this.request({
			command: 'models/discover',
			providers,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to discover models');
		}

		return response.result as {
			models: Array<{
				modelId: string;
				contextWindow: number;
				maxOutputTokens?: number;
				provider: string;
				capabilities?: string[];
				costPer1kTokens?: { input: number; output: number };
				discoveredAt: number;
			}>;
			count: number;
			providers: number;
		};
	}

	// ========================================================================
	// RAG Module Methods (batched parallel source loading)
	// ========================================================================

	/**
	 * Compose RAG context from multiple sources in a single batched call.
	 * Rust processes all sources in parallel using Rayon.
	 *
	 * This replaces N sequential IPC calls with ONE call:
	 * - Memory recall
	 * - Consciousness context
	 * - Scene (games/VR)
	 * - Project (code context)
	 * - Custom sources
	 *
	 * @param personaId - The persona's UUID
	 * @param roomId - Current room/context ID
	 * @param sources - Array of source requests with typed params
	 * @param queryText - Optional query for semantic search
	 * @param totalBudget - Total token budget across all sources
	 */
	async ragCompose(
		personaId: string,
		roomId: string,
		sources: RagSourceRequest[],
		queryText?: string,
		totalBudget: number = 2000
	): Promise<RagComposeResult> {
		const response = await this.request({
			command: 'rag/compose',
			persona_id: personaId,
			room_id: roomId,
			sources,
			query_text: queryText ?? null,
			total_budget: totalBudget,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to compose RAG context');
		}

		return response.result as RagComposeResult;
	}

	// ========================================================================
	// Search Module Methods (absorbs standalone search worker)
	// ========================================================================

	/**
	 * List available search algorithms
	 */
	async searchList(): Promise<string[]> {
		const response = await this.request({
			command: 'search/list',
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to list search algorithms');
		}

		return response.result?.algorithms || [];
	}

	/**
	 * Execute text search using specified algorithm
	 */
	async searchExecute(
		query: string,
		corpus: string[],
		algorithm: string = 'bm25',
		params?: Record<string, unknown>
	): Promise<{ algorithm: string; scores: number[]; rankedIndices: number[] }> {
		const response = await this.request({
			command: 'search/execute',
			algorithm,
			query,
			corpus,
			params: params ?? null,
		});

		if (!response.success) {
			throw new Error(response.error || 'Search execution failed');
		}

		return {
			algorithm: response.result?.algorithm || algorithm,
			scores: response.result?.scores || [],
			rankedIndices: response.result?.rankedIndices || [],
		};
	}

	/**
	 * Vector similarity search using cosine similarity
	 */
	async searchVector(
		queryVector: number[],
		corpusVectors: number[][],
		normalize: boolean = true,
		threshold: number = 0.0
	): Promise<{ scores: number[]; rankedIndices: number[] }> {
		const response = await this.request({
			command: 'search/vector',
			queryVector,
			corpusVectors,
			normalize,
			threshold,
		});

		if (!response.success) {
			throw new Error(response.error || 'Vector search failed');
		}

		return {
			scores: response.result?.scores || [],
			rankedIndices: response.result?.rankedIndices || [],
		};
	}

	/**
	 * Get algorithm parameters and current values
	 */
	async searchParams(algorithm: string): Promise<{ params: string[]; values: Record<string, unknown> }> {
		const response = await this.request({
			command: 'search/params',
			algorithm,
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to get search params');
		}

		return {
			params: response.result?.params || [],
			values: response.result?.values || {},
		};
	}

	// ========================================================================
	// Runtime Module Methods (system monitoring & observability)
	// ========================================================================

	/**
	 * List all registered modules with their configurations.
	 * Returns module names, priorities, command prefixes, and thread settings.
	 */
	async runtimeList(): Promise<{
		modules: Array<{
			name: string;
			priority: string;
			command_prefixes: string[];
			needs_dedicated_thread: boolean;
			max_concurrency: number;
		}>;
		count: number;
	}> {
		const response = await this.request({
			command: 'runtime/list',
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to list runtime modules');
		}

		return response.result as {
			modules: Array<{
				name: string;
				priority: string;
				command_prefixes: string[];
				needs_dedicated_thread: boolean;
				max_concurrency: number;
			}>;
			count: number;
		};
	}

	/**
	 * Get performance metrics for all modules.
	 * Returns aggregate stats including command counts, avg latency, percentiles.
	 */
	async runtimeMetricsAll(): Promise<{
		modules: Array<{
			moduleName: string;
			totalCommands: number;
			avgTimeMs: number;
			slowCommandCount: number;
			p50Ms: number;
			p95Ms: number;
			p99Ms: number;
		}>;
		count: number;
	}> {
		const response = await this.request({
			command: 'runtime/metrics/all',
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to get runtime metrics');
		}

		// Convert bigint fields to number (ts-rs exports u64 as bigint)
		const result = response.result as { modules: any[]; count: number };
		return {
			modules: result.modules.map((m: any) => ({
				moduleName: m.moduleName,
				totalCommands: Number(m.totalCommands),
				avgTimeMs: Number(m.avgTimeMs),
				slowCommandCount: Number(m.slowCommandCount),
				p50Ms: Number(m.p50Ms),
				p95Ms: Number(m.p95Ms),
				p99Ms: Number(m.p99Ms),
			})),
			count: result.count,
		};
	}

	/**
	 * Get performance metrics for a specific module.
	 */
	async runtimeMetricsModule(moduleName: string): Promise<{
		moduleName: string;
		totalCommands: number;
		avgTimeMs: number;
		slowCommandCount: number;
		p50Ms: number;
		p95Ms: number;
		p99Ms: number;
	}> {
		const response = await this.request({
			command: 'runtime/metrics/module',
			module: moduleName,
		});

		if (!response.success) {
			throw new Error(response.error || `Failed to get metrics for module: ${moduleName}`);
		}

		// Convert bigint fields to number
		const m = response.result as any;
		return {
			moduleName: m.moduleName,
			totalCommands: Number(m.totalCommands),
			avgTimeMs: Number(m.avgTimeMs),
			slowCommandCount: Number(m.slowCommandCount),
			p50Ms: Number(m.p50Ms),
			p95Ms: Number(m.p95Ms),
			p99Ms: Number(m.p99Ms),
		};
	}

	/**
	 * Get list of recent slow commands (>50ms) across all modules.
	 * Sorted by total_ms descending.
	 */
	async runtimeMetricsSlow(): Promise<{
		slow_commands: Array<{
			module: string;
			command: string;
			total_ms: number;
			execute_ms: number;
			queue_ms: number;
		}>;
		count: number;
		threshold_ms: number;
	}> {
		const response = await this.request({
			command: 'runtime/metrics/slow',
		});

		if (!response.success) {
			throw new Error(response.error || 'Failed to get slow commands');
		}

		return response.result as {
			slow_commands: Array<{
				module: string;
				command: string;
				total_ms: number;
				execute_ms: number;
				queue_ms: number;
			}>;
			count: number;
			threshold_ms: number;
		};
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
