/**
 * RustCoreIPC Cognition Module - Persona cognition engine methods
 */

import type { RustCoreIPCClientBase } from './base';
import type {
	InboxMessageRequest,
	CognitionDecision,
	PriorityScore,
	PersonaState,
	ActivityDomain,
	TextSimilarityResult,
	SemanticLoopResult,
	ConversationMessage,
	ValidationResult,
	MentionCheckResult,
	CleanedResponse,
	FullEvaluateRequest,
	FullEvaluateResult,
	SleepMode,
	ModelSelectionResult,
	AdapterInfo,
	GenomeAdapterInfo,
	GenomePagingState,
	ActivateSkillResult,
	AdequacyResult,
	DomainClassification,
	CoverageReport,
	QualityScore,
} from '../../../../shared/generated';
import type { PersonaResponse } from '../../../../shared/generated/cognition/PersonaResponse';
import type { Signal } from '../../../../shared/generated/recipe/Signal';
import type { PersonaContext } from '../../../../shared/generated/recipe/PersonaContext';

/**
 * Caller-supplied input for `cognition/respond`.
 *
 * Two fields:
 * - `signal` — host's raw event (chat message, video frame, code diff,
 *   game tick). The Rust side projects it into the cognition layer's
 *   internal RespondInput via `cognition_io::build_respond_input`.
 * - `personaContext` — per-persona stable state (identity, model,
 *   capabilities, recent history). Built from the room/persona before
 *   each turn.
 *
 * Both `Signal` and `PersonaContext` are ts-rs generated from the Rust
 * source of truth (persona/cognition_io.rs). Hosts construct them via
 * normal TS object literals; the wire format is camelCase JSON.
 *
 * Recipe selection is NOT in this payload — recipes are JSON data
 * walked by whatever wraps this call (today: nothing — chat dispatches
 * directly; future: a small walker that interprets recipe pipelines
 * for non-chat hosts). The cognition layer just runs the projection
 * and `respond()`.
 */
export interface PersonaRespondRequest {
	signal: Signal;
	personaContext: PersonaContext;
}

// ============================================================================
// Mixin
// ============================================================================

export interface CognitionMixin {
	inboxCreate(personaId: string): Promise<void>;
	cognitionCreateEngine(personaId: string, personaName: string): Promise<void>;
	cognitionCalculatePriority(
		personaId: string,
		content: string,
		senderType: 'human' | 'persona' | 'agent' | 'system',
		isVoice: boolean,
		roomId: string,
		timestamp: number
	): Promise<PriorityScore>;
	cognitionFastPathDecision(personaId: string, message: InboxMessageRequest): Promise<CognitionDecision>;
	cognitionEnqueueMessage(personaId: string, message: InboxMessageRequest): Promise<void>;
	cognitionGetState(personaId: string): Promise<PersonaState & { service_cadence_ms: number }>;
	cognitionTextSimilarity(text1: string, text2: string): Promise<TextSimilarityResult>;
	cognitionCheckSemanticLoop(responseText: string, history: ConversationMessage[], maxHistory?: number): Promise<SemanticLoopResult>;
	cognitionValidateResponse(
		personaId: string,
		responseText: string,
		hasToolCalls: boolean,
		conversationHistory?: ConversationMessage[]
	): Promise<ValidationResult>;
	cognitionCheckMentions(
		messageText: string,
		personaDisplayName: string,
		personaUniqueId: string
	): Promise<MentionCheckResult>;
	cognitionCleanResponse(responseText: string): Promise<CleanedResponse>;
	cognitionFullEvaluate(request: FullEvaluateRequest): Promise<FullEvaluateResult>;
	cognitionTrackResponse(personaId: string, roomId: string): Promise<{ tracked: boolean; response_count: number }>;
	cognitionSetSleepMode(personaId: string, mode: SleepMode, reason?: string, durationMinutes?: number): Promise<{ set: boolean; previous_mode: string; new_mode: string; wake_at_ms: number | null }>;
	cognitionConfigureRateLimiter(personaId: string, minSeconds?: number, maxResponses?: number): Promise<{ configured: boolean }>;
	cognitionSelectModel(personaId: string, baseModel: string, taskDomain?: string): Promise<ModelSelectionResult>;
	cognitionSyncAdapters(personaId: string, adapters: AdapterInfo[]): Promise<{ synced: boolean; adapter_count: number }>;
	cognitionGenomeActivateSkill(personaId: string, skillName: string, memoryBudgetMb?: number): Promise<ActivateSkillResult>;
	cognitionGenomeSync(personaId: string, adapters: GenomeAdapterInfo[], memoryBudgetMb?: number): Promise<{ synced: boolean; adapter_count: number; active_count: number; memory_used_mb: number; memory_pressure: number }>;
	cognitionGenomeState(personaId: string): Promise<GenomePagingState>;
	cognitionCheckAdequacy(originalText: string, responses: Array<{ sender_name: string; text: string }>): Promise<AdequacyResult>;
	cognitionHasEvaluated(personaId: string, messageId: string): Promise<boolean>;
	cognitionMarkEvaluated(personaId: string, messageId: string): Promise<void>;
	cognitionClassifyDomain(personaId: string, text: string): Promise<DomainClassification>;
	cognitionSyncDomainClassifier(personaId: string): Promise<{ synced: boolean; total_domains: number; covered_domains: number }>;
	cognitionRegisterDomainKeywords(personaId: string, domain: string, keywords: string[]): Promise<{ registered: boolean; domain: string; keywords_added: number }>;
	cognitionGenomeRecordActivity(personaId: string, domain: string, success: boolean): Promise<{ recorded: boolean }>;
	cognitionGenomeCoverageReport(personaId: string): Promise<CoverageReport>;
	cognitionScoreInteraction(input: string, output: string, feedback?: string, taskSuccess?: boolean): Promise<QualityScore>;
	cognitionCacheMessage(personaId: string, roomId: string, messageId: string, senderId: string, senderType: string, senderName: string, content: string, timestamp: number): Promise<void>;
	cognitionCheckContentDedup(personaId: string, roomId: string, content: string): Promise<{ is_duplicate: boolean; check_time_us: number }>;
	cognitionRecordContent(personaId: string, roomId: string, content: string): Promise<void>;

	/**
	 * SHARED COGNITION — single external entry point for the per-persona
	 * response cycle. Rust runs analysis (cached) → score → prompt assembly
	 * → inference → <think>-strip → emits cognition:think-block events.
	 *
	 * Returns either Silent { reason } (first-class outcome — persona
	 * considered the message and chose not to speak) or Spoke { text,
	 * model_used, inference_ms, total_ms, think_blocks_emitted }. Caller
	 * (PRG.ts shim) is responsible for posting the Spoke text; Silent
	 * outcomes can be logged for trainability but should NOT post empty
	 * messages.
	 *
	 * See docs/architecture/SHARED-COGNITION.md.
	 */
	cognitionPersonaRespond(req: PersonaRespondRequest): Promise<PersonaResponse>;
}

export function CognitionMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements CognitionMixin {
		/**
		 * Create an inbox for a persona
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

		/**
		 * Create cognition engine for a persona
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
		 * Fast-path gating decision
		 */
		async cognitionFastPathDecision(
			personaId: string,
			messageRequest: InboxMessageRequest
		): Promise<CognitionDecision> {
			const response = await this.request({
				command: 'cognition/fast-path-decision',
				persona_id: personaId,
				message: messageRequest,
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
		 * Get persona cognitive state
		 */
		async cognitionGetState(personaId: string): Promise<PersonaState & { service_cadence_ms: number }> {
			const response = await this.request({
				command: 'cognition/get-state',
				persona_id: personaId,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to get cognition state');
			}

			return response.result as PersonaState & { service_cadence_ms: number };
		}

		/**
		 * Unified text similarity — both char-bigram and word-ngram Jaccard in one call.
		 * Replaces 3 duplicate TS implementations.
		 */
		async cognitionTextSimilarity(text1: string, text2: string): Promise<TextSimilarityResult> {
			const response = await this.request({
				command: 'cognition/text-similarity',
				text1,
				text2,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to compute text similarity');
			}

			return response.result as TextSimilarityResult;
		}

		/**
		 * Check if a response is semantically looping against conversation history.
		 * Blocks at 95% similarity, warns at 80%.
		 */
		async cognitionCheckSemanticLoop(
			responseText: string,
			history: ConversationMessage[],
			maxHistory?: number
		): Promise<SemanticLoopResult> {
			const response = await this.request({
				command: 'cognition/check-semantic-loop',
				response_text: responseText,
				history,
				max_history: maxHistory ?? 10,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to check semantic loop');
			}

			return response.result as SemanticLoopResult;
		}

		/**
		 * Combined mention detection: is_persona_mentioned + has_directed_mention.
		 * ONE IPC call replaces 2 separate string checks.
		 */
		async cognitionCheckMentions(
			messageText: string,
			personaDisplayName: string,
			personaUniqueId: string
		): Promise<MentionCheckResult> {
			const response = await this.request({
				command: 'cognition/check-mentions',
				message_text: messageText,
				persona_display_name: personaDisplayName,
				persona_unique_id: personaUniqueId,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to check mentions');
			}

			return response.result as MentionCheckResult;
		}

		/**
		 * Clean AI response by stripping unwanted prefixes (timestamps, names, markdown).
		 */
		async cognitionCleanResponse(responseText: string): Promise<CleanedResponse> {
			const response = await this.request({
				command: 'cognition/clean-response',
				response_text: responseText,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to clean response');
			}

			return response.result as CleanedResponse;
		}

		/**
		 * Combined validation: garbage + response loop + truncated tool + semantic loop.
		 * ONE IPC call replaces 4 separate validation gates.
		 */
		async cognitionValidateResponse(
			personaId: string,
			responseText: string,
			hasToolCalls: boolean,
			conversationHistory?: ConversationMessage[]
		): Promise<ValidationResult> {
			const response = await this.request({
				command: 'cognition/validate-response',
				persona_id: personaId,
				response_text: responseText,
				has_tool_calls: hasToolCalls,
				conversation_history: conversationHistory ?? [],
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to validate response');
			}

			return response.result as ValidationResult;
		}

		/**
		 * Unified evaluation gate — ONE IPC call replaces 5 sequential TS gates.
		 * Gates: response_cap → mention → rate_limit → sleep_mode → directed_mention → fast_path
		 */
		async cognitionFullEvaluate(request: FullEvaluateRequest): Promise<FullEvaluateResult> {
			const response = await this.request({
				command: 'cognition/full-evaluate',
				...request,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to run full evaluate');
			}

			return response.result as FullEvaluateResult;
		}

		/**
		 * Track a response for rate limiting state in Rust.
		 */
		async cognitionTrackResponse(
			personaId: string,
			roomId: string
		): Promise<{ tracked: boolean; response_count: number }> {
			const response = await this.request({
				command: 'cognition/track-response',
				persona_id: personaId,
				room_id: roomId,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to track response');
			}

			return response.result as { tracked: boolean; response_count: number };
		}

		/**
		 * Set voluntary sleep mode for a persona.
		 */
		async cognitionSetSleepMode(
			personaId: string,
			mode: SleepMode,
			reason?: string,
			durationMinutes?: number
		): Promise<{ set: boolean; previous_mode: string; new_mode: string; wake_at_ms: number | null }> {
			const response = await this.request({
				command: 'cognition/set-sleep-mode',
				persona_id: personaId,
				mode,
				reason,
				duration_minutes: durationMinutes,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to set sleep mode');
			}

			return response.result as { set: boolean; previous_mode: string; new_mode: string; wake_at_ms: number | null };
		}

		/**
		 * Configure rate limiter parameters for a persona.
		 */
		async cognitionConfigureRateLimiter(
			personaId: string,
			minSeconds?: number,
			maxResponses?: number
		): Promise<{ configured: boolean }> {
			const response = await this.request({
				command: 'cognition/configure-rate-limiter',
				persona_id: personaId,
				min_seconds_between_responses: minSeconds,
				max_responses_per_session: maxResponses,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to configure rate limiter');
			}

			return response.result as { configured: boolean };
		}

		/**
		 * Select the best model for a persona using 4-tier priority chain.
		 * Tier 1: trait-specific adapter → 2: current → 3: any → 4: base model.
		 */
		async cognitionSelectModel(
			personaId: string,
			baseModel: string,
			taskDomain?: string
		): Promise<ModelSelectionResult> {
			const response = await this.request({
				command: 'cognition/select-model',
				persona_id: personaId,
				base_model: baseModel,
				...(taskDomain && { task_domain: taskDomain }),
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to select model');
			}

			return response.result as ModelSelectionResult;
		}

		/**
		 * Sync adapter registry from TypeScript genome state to Rust.
		 * Full replacement — sends all known adapters.
		 */
		async cognitionSyncAdapters(
			personaId: string,
			adapters: AdapterInfo[]
		): Promise<{ synced: boolean; adapter_count: number }> {
			const response = await this.request({
				command: 'cognition/sync-adapters',
				persona_id: personaId,
				adapters,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to sync adapters');
			}

			return response.result as { synced: boolean; adapter_count: number };
		}

		/**
		 * Genome paging: decide what to evict/load for a skill activation.
		 * Rust makes the decision, TypeScript executes the GPU ops.
		 */
		async cognitionGenomeActivateSkill(
			personaId: string,
			skillName: string,
			memoryBudgetMb?: number
		): Promise<ActivateSkillResult> {
			const response = await this.request({
				command: 'cognition/genome-activate-skill',
				persona_id: personaId,
				skill_name: skillName,
				...(memoryBudgetMb !== undefined && { memory_budget_mb: memoryBudgetMb }),
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to activate skill');
			}

			return response.result as ActivateSkillResult;
		}

		/**
		 * Sync full genome adapter state from TypeScript to Rust.
		 */
		async cognitionGenomeSync(
			personaId: string,
			adapters: GenomeAdapterInfo[],
			memoryBudgetMb?: number
		): Promise<{ synced: boolean; adapter_count: number; active_count: number; memory_used_mb: number; memory_pressure: number }> {
			const response = await this.request({
				command: 'cognition/genome-sync',
				persona_id: personaId,
				adapters,
				...(memoryBudgetMb !== undefined && { memory_budget_mb: memoryBudgetMb }),
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to sync genome');
			}

			return response.result as { synced: boolean; adapter_count: number; active_count: number; memory_used_mb: number; memory_pressure: number };
		}

		/**
		 * Get current genome paging state from Rust.
		 */
		async cognitionGenomeState(personaId: string): Promise<GenomePagingState> {
			const response = await this.request({
				command: 'cognition/genome-state',
				persona_id: personaId,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to get genome state');
			}

			return response.result as GenomePagingState;
		}

		/**
		 * Batch check if other AIs already answered a question.
		 * ONE IPC call replaces N individual textSimilarity calls.
		 */
		async cognitionCheckAdequacy(
			originalText: string,
			responses: Array<{ sender_name: string; text: string }>
		): Promise<AdequacyResult> {
			const response = await this.request({
				command: 'cognition/check-adequacy',
				original_text: originalText,
				responses,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to check adequacy');
			}

			return response.result as AdequacyResult;
		}

		/**
		 * Check if a message has already been evaluated (deduplication).
		 */
		async cognitionHasEvaluated(personaId: string, messageId: string): Promise<boolean> {
			const response = await this.request({
				command: 'cognition/has-evaluated',
				persona_id: personaId,
				message_id: messageId,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to check evaluated status');
			}

			return (response.result as { evaluated: boolean }).evaluated;
		}

		/**
		 * Mark a message as evaluated (deduplication).
		 */
		async cognitionMarkEvaluated(personaId: string, messageId: string): Promise<void> {
			const response = await this.request({
				command: 'cognition/mark-evaluated',
				persona_id: personaId,
				message_id: messageId,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to mark message as evaluated');
			}
		}

		/**
		 * Classify text into a skill domain using adapter-aware keyword scoring.
		 * Returns domain, confidence, and matching adapter (if any).
		 */
		async cognitionClassifyDomain(personaId: string, text: string): Promise<DomainClassification> {
			const response = await this.request({
				command: 'cognition/classify-domain',
				persona_id: personaId,
				text,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to classify domain');
			}

			return response.result as DomainClassification;
		}

		/**
		 * Sync domain classifier with current adapter state.
		 * Call after genome changes (training complete, adapter registered).
		 */
		async cognitionSyncDomainClassifier(personaId: string): Promise<{ synced: boolean; total_domains: number; covered_domains: number }> {
			const response = await this.request({
				command: 'cognition/sync-domain-classifier',
				persona_id: personaId,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to sync domain classifier');
			}

			return response.result as { synced: boolean; total_domains: number; covered_domains: number };
		}

		/**
		 * Register new keywords for a domain (e.g., from academy curriculum).
		 */
		async cognitionRegisterDomainKeywords(personaId: string, domain: string, keywords: string[]): Promise<{ registered: boolean; domain: string; keywords_added: number }> {
			const response = await this.request({
				command: 'cognition/register-domain-keywords',
				persona_id: personaId,
				domain,
				keywords,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to register domain keywords');
			}

			return response.result as { registered: boolean; domain: string; keywords_added: number };
		}

		/**
		 * Record domain activity for gap detection.
		 * Call after every inference response.
		 */
		async cognitionGenomeRecordActivity(personaId: string, domain: string, success: boolean): Promise<{ recorded: boolean }> {
			const response = await this.request({
				command: 'cognition/genome-record-activity',
				persona_id: personaId,
				domain,
				success,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to record activity');
			}

			return response.result as { recorded: boolean };
		}

		/**
		 * Get coverage report: which domains have adapters, which are gaps.
		 */
		async cognitionGenomeCoverageReport(personaId: string): Promise<CoverageReport> {
			const response = await this.request({
				command: 'cognition/genome-coverage-report',
				persona_id: personaId,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to get coverage report');
			}

			return response.result as CoverageReport;
		}

		/**
		 * Score interaction quality for training data selection.
		 */
		async cognitionScoreInteraction(input: string, output: string, feedback?: string, taskSuccess?: boolean): Promise<QualityScore> {
			const response = await this.request({
				command: 'cognition/score-interaction',
				input,
				output,
				...(feedback !== undefined && { feedback }),
				...(taskSuccess !== undefined && { task_success: taskSuccess }),
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to score interaction');
			}

			return response.result as QualityScore;
		}

		/**
		 * Cache a message for echo chamber detection and post-inference adequacy.
		 * Called by TS event handler when messages arrive — feeds the Rust-side ring buffer.
		 */
		async cognitionCacheMessage(
			personaId: string,
			roomId: string,
			messageId: string,
			senderId: string,
			senderType: string,
			senderName: string,
			content: string,
			timestamp: number,
		): Promise<void> {
			await this.request({
				command: 'cognition/cache-message',
				persona_id: personaId,
				room_id: roomId,
				message_id: messageId,
				sender_id: senderId,
				sender_type: senderType,
				sender_name: senderName,
				content,
				timestamp,
			});
		}

		/**
		 * Check if content is a duplicate (same hash posted to same room within window).
		 */
		async cognitionCheckContentDedup(
			personaId: string,
			roomId: string,
			content: string,
		): Promise<{ is_duplicate: boolean; check_time_us: number }> {
			const response = await this.request({
				command: 'cognition/check-content-dedup',
				persona_id: personaId,
				room_id: roomId,
				content,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to check content dedup');
			}

			return response.result as { is_duplicate: boolean; check_time_us: number };
		}

		/**
		 * Record content for future dedup checks.
		 */
		async cognitionRecordContent(
			personaId: string,
			roomId: string,
			content: string,
		): Promise<void> {
			await this.request({
				command: 'cognition/record-content',
				persona_id: personaId,
				room_id: roomId,
				content,
			});
		}

		/**
		 * Per-persona response cycle (shared cognition pipeline).
		 * Single IPC call → Rust does analysis (cached) + scoring + prompt
		 * assembly + inference + <think> stripping. Returns the structured
		 * PersonaResponse that the caller posts (or logs, if Silent).
		 */
		async cognitionPersonaRespond(req: PersonaRespondRequest): Promise<PersonaResponse> {
			// Timeout split by provider class:
			//   cloud (anthropic/openai/groq/…) → 180s. A healthy cloud call
			//   completes in 2–10s; at 180s something is genuinely wrong and
			//   we want the loud failure.
			//   local (in-process llama.cpp / DMR) → 300s. The persona
			//   pipeline runs analyze (qwen3.5 reasoning preamble + JSON,
			//   30–60s alone) + score + assemble + inference + strip, and
			//   under 3-way concurrent the llamacpp scheduler's per-seq
			//   throughput drops to ~1.3 tok/s → a 1500+ token reasoning
			//   response legitimately takes 200–280s. Tripping 180s there
			//   was the WRONG signal: inference was working, just queued.
			//   300s still surfaces genuine hangs (model crashed / infinite
			//   reasoning) loudly.
			//
			// Streaming IPC (return tokens incrementally, no end-to-end cap)
			// is the architecturally-right next step — filed as follow-up,
			// not included in this change.
			const model = req.personaContext.model;
			const isLocal = model.startsWith('continuum-ai/') || model.startsWith('qwen2-vl');
			const COGNITION_RESPOND_TIMEOUT_MS = isLocal ? 300_000 : 180_000;

			// Wire shape: { signal, personaContext }. Rust projects via
			// cognition_io::build_respond_input, runs respond(), returns
			// the response. No recipe-name field — recipes are JSON
			// data walked above this layer.
			const { response } = await this.requestFull({
				command: 'cognition/respond',
				signal: req.signal,
				personaContext: req.personaContext,
			}, COGNITION_RESPOND_TIMEOUT_MS);

			if (!response.success) {
				throw new Error(response.error || 'Failed to run persona/respond');
			}

			return response.result as PersonaResponse;
		}
	};
}
