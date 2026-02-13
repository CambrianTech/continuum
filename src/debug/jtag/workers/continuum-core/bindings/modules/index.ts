/**
 * RustCoreIPC Modules - Modular IPC client components
 *
 * Each module provides domain-specific methods via TypeScript mixins.
 * Import the composed RustCoreIPCClient from ../RustCoreIPC.ts for full functionality.
 */

// Base client with core connection logic
export { RustCoreIPCClientBase, resolveSocketPath, getContinuumCoreSocketPath } from './base';
export type { IPCResponse, IPCJsonResponse } from './base';

// Domain mixins
export { VoiceMixin } from './voice';
export type { VoiceMixin as VoiceMixinInterface, VoiceParticipant, UtteranceEvent } from './voice';

export { CognitionMixin } from './cognition';
export type { CognitionMixin as CognitionMixinInterface } from './cognition';

export { ChannelMixin } from './channel';
export type { ChannelMixin as ChannelMixinInterface } from './channel';

export { MemoryMixin } from './memory';
export type { MemoryMixin as MemoryMixinInterface } from './memory';

export { CodeMixin } from './code';
export type { CodeMixin as CodeMixinInterface } from './code';

export { SearchMixin } from './search';
export type { SearchMixin as SearchMixinInterface } from './search';

export { RagMixin } from './rag';
export type { RagMixin as RagMixinInterface } from './rag';

export { ModelsMixin } from './models';
export type { ModelsMixin as ModelsMixinInterface, DiscoveredModel } from './models';

export { AIMixin } from './ai';
export type { AIMixin as AIMixinInterface, AIGenerateParams, AIGenerateResult } from './ai';

export { EmbeddingMixin } from './embedding';
export type { EmbeddingMixin as EmbeddingMixinInterface, EmbeddingResult, SimilarityResult, TopKResult, ClusterResult, TopKResponse } from './embedding';

export { RuntimeMixin } from './runtime';
export type { RuntimeMixin as RuntimeMixinInterface, ModuleInfo, ModuleMetrics, SlowCommand } from './runtime';

/**
 * Compose all mixins into a single client class.
 * Usage: const Client = composeClient(RustCoreIPCClientBase);
 */
export function composeClient<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return RuntimeMixin(
		EmbeddingMixin(
			AIMixin(
				ModelsMixin(
					RagMixin(
						SearchMixin(
							CodeMixin(
								MemoryMixin(
									ChannelMixin(
										CognitionMixin(
											VoiceMixin(Base)
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
}
