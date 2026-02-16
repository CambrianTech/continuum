/**
 * Continuum Core IPC Client - TypeScript <-> Rust via Unix Socket
 *
 * MODULAR ARCHITECTURE:
 * The client is composed from domain-specific modules in ./modules/
 * Each module provides methods for a specific domain (voice, code, memory, etc.)
 *
 * Binary framing protocol (length-prefixed):
 * - Requests: Newline-delimited JSON (TypeScript → Rust)
 * - Responses: [4 bytes u32 BE length][payload] (Rust → TypeScript)
 *   - JSON-only: payload = JSON bytes
 *   - Binary: payload = JSON bytes + \0 + raw binary bytes
 */

// Re-export utilities from base
export { resolveSocketPath, getContinuumCoreSocketPath } from './modules/base';
export type { IPCResponse, IPCJsonResponse } from './modules/base';

// Re-export types from modules
export type { VoiceParticipant, UtteranceEvent, VoiceSynthesizeResult } from './modules/voice';
export type { DiscoveredModel, ProviderConfig, ModelsDiscoverResult } from './modules/models';
export type { AIGenerateParams, AIGenerateResult } from './modules/ai';
export type { EmbeddingResult, SimilarityResult, TopKResult, TopKResponse, ClusterResult } from './modules/embedding';
export type { SearchExecuteResult, SearchVectorResult } from './modules/search';
export type { ChannelEnqueueResult, ChannelDequeueResult, ChannelServiceCycleResult, ChannelServiceCycleFullResult } from './modules/channel';
export type { ModuleInfo, ModuleMetrics, SlowCommand } from './modules/runtime';
export type {
	SentinelHandle,
	SentinelRunParams,
	SentinelRunResult,
	SentinelStatusResult,
	SentinelListResult,
	SentinelLogsListResult,
	SentinelLogsReadResult,
	SentinelLogsTailResult,
	LogStreamInfo,
} from './modules/sentinel';

// Import base and all mixins
import { RustCoreIPCClientBase, getContinuumCoreSocketPath } from './modules/base';
import { VoiceMixin } from './modules/voice';
import { CognitionMixin } from './modules/cognition';
import { ChannelMixin } from './modules/channel';
import { MemoryMixin } from './modules/memory';
import { CodeMixin } from './modules/code';
import { SearchMixin } from './modules/search';
import { RagMixin } from './modules/rag';
import { ModelsMixin } from './modules/models';
import { AIMixin } from './modules/ai';
import { EmbeddingMixin } from './modules/embedding';
import { RuntimeMixin } from './modules/runtime';
import { SentinelMixin } from './modules/sentinel';
import { ToolParsingMixin } from './modules/tool_parsing';

// Re-export types from shared/generated (used by consumers)
export type {
	InboxMessageRequest,
	CognitionDecision,
	PriorityScore,
	PersonaState,
	ActivityDomain,
	ChannelRegistryStatus,
	ChannelEnqueueRequest,
	ServiceCycleResult,
	FullEvaluateRequest,
	FullEvaluateResult,
	SleepMode,
	ModelSelectionResult,
	AdapterInfo,
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
	ShellExecuteResponse,
	ShellPollResponse,
	ShellSessionInfo,
	ShellWatchResponse,
	SentinelRule,
	ToolParseResult,
	ParsedToolCall,
	CorrectedToolCall,
} from '../../../shared/generated';

// Re-export memory types
export type { CorpusMemory } from './CorpusMemory';
export type { CorpusTimelineEvent } from './CorpusTimelineEvent';
export type { LoadCorpusResponse } from './LoadCorpusResponse';
export type { MemoryRecallResponse } from './MemoryRecallResponse';
export type { MultiLayerRecallRequest } from './MultiLayerRecallRequest';
export type { ConsciousnessContextResponse } from './ConsciousnessContextResponse';

// Re-export RAG types
export type { RagSourceRequest, RagComposeResult } from '../../../shared/generated/rag';

/**
 * Compose all mixins into the full client class.
 * Order matters for TypeScript type inference.
 */
const ComposedClient = ToolParsingMixin(
	SentinelMixin(
		RuntimeMixin(
			EmbeddingMixin(
				AIMixin(
					ModelsMixin(
						RagMixin(
							SearchMixin(
								CodeMixin(
									MemoryMixin(
										ChannelMixin(
											CognitionMixin(
												VoiceMixin(RustCoreIPCClientBase)
											)
										)
									)
								)
							)
						)
					)
				)
			)
		)
	)
);

/**
 * Full RustCoreIPCClient with all domain methods.
 *
 * Usage:
 *   const client = RustCoreIPCClient.getInstance();
 *   await client.voiceRegisterSession(...);
 *   await client.codeRead(...);
 *   await client.aiGenerate(...);
 */
export class RustCoreIPCClient extends ComposedClient {
	/** Singleton instance */
	private static _singletonInstance: RustCoreIPCClient | null = null;
	private static _singletonPromise: Promise<RustCoreIPCClient> | null = null;

	constructor(socketPath: string = getContinuumCoreSocketPath()) {
		super(socketPath);
	}

	/**
	 * Get shared singleton instance (auto-connects on first call).
	 */
	static getInstance(): RustCoreIPCClient {
		if (!RustCoreIPCClient._singletonInstance) {
			RustCoreIPCClient._singletonInstance = new RustCoreIPCClient();
			RustCoreIPCClient._singletonPromise = RustCoreIPCClient._singletonInstance.connect()
				.then(() => RustCoreIPCClient._singletonInstance!);
		}
		return RustCoreIPCClient._singletonInstance;
	}

	/**
	 * Get shared singleton instance, waiting for connection.
	 */
	static async getInstanceAsync(): Promise<RustCoreIPCClient> {
		if (!RustCoreIPCClient._singletonInstance) {
			RustCoreIPCClient.getInstance();
		}
		await RustCoreIPCClient._singletonPromise;
		return RustCoreIPCClient._singletonInstance!;
	}
}

// Default export for convenience
export default RustCoreIPCClient;
