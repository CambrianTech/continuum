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
} from '../../../../shared/generated';

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
	};
}
