/**
 * RustCoreIPC Channel Module - RTOS channel/queue methods
 */

import type { RustCoreIPCClientBase } from './base';
import type {
	ChannelRegistryStatus,
	ChannelEnqueueRequest,
	ActivityDomain,
	CognitionDecision,
} from '../../../../shared/generated';

// ============================================================================
// Types
// ============================================================================

export interface ChannelEnqueueResult {
	routed_to: ActivityDomain;
	status: ChannelRegistryStatus;
}

export interface ChannelDequeueResult {
	item: any | null;
	has_more: boolean;
}

export interface ChannelServiceCycleResult {
	should_process: boolean;
	item: any | null;
	channel: ActivityDomain | null;
	wait_ms: number;
	stats: ChannelRegistryStatus;
}

export interface ChannelServiceCycleFullResult extends ChannelServiceCycleResult {
	decision: CognitionDecision | null;
}

// ============================================================================
// Mixin
// ============================================================================

export interface ChannelMixin {
	channelEnqueue(personaId: string, item: ChannelEnqueueRequest): Promise<ChannelEnqueueResult>;
	channelDequeue(personaId: string, domain?: ActivityDomain): Promise<ChannelDequeueResult>;
	channelStatus(personaId: string): Promise<ChannelRegistryStatus>;
	channelServiceCycle(personaId: string): Promise<ChannelServiceCycleResult>;
	channelServiceCycleFull(personaId: string): Promise<ChannelServiceCycleFullResult>;
	channelClear(personaId: string): Promise<void>;
}

export function ChannelMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements ChannelMixin {
		/**
		 * Enqueue an item into the channel system.
		 * Item is routed to the correct domain channel (AUDIO/CHAT/BACKGROUND).
		 */
		async channelEnqueue(
			personaId: string,
			item: ChannelEnqueueRequest
		): Promise<ChannelEnqueueResult> {
			const response = await this.request({
				command: 'channel/enqueue',
				persona_id: personaId,
				item,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to enqueue channel item');
			}

			return response.result as ChannelEnqueueResult;
		}

		/**
		 * Dequeue highest-priority item from a specific domain or any domain.
		 */
		async channelDequeue(
			personaId: string,
			domain?: ActivityDomain
		): Promise<ChannelDequeueResult> {
			const response = await this.request({
				command: 'channel/dequeue',
				persona_id: personaId,
				domain: domain ?? null,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to dequeue channel item');
			}

			return response.result as ChannelDequeueResult;
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
		 * Run a service cycle - atomically selects next item to process.
		 */
		async channelServiceCycle(personaId: string): Promise<ChannelServiceCycleResult> {
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
		 */
		async channelServiceCycleFull(personaId: string): Promise<ChannelServiceCycleFullResult> {
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
	};
}
