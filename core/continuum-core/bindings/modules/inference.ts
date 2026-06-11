/**
 * RustCoreIPC Inference Module — inference-side system facts
 *
 * The single source of truth for local-inference concurrency. Previously
 * TypeScript (`InferenceCoordinator.localInferenceCapacity`) and Rust
 * (`local_inference_capacity`) independently computed the same RAM-based
 * formula — prone to drift. Now TS reads this from Rust.
 *
 * See issue #887.
 */

import type { RustCoreIPCClientBase } from './base';

export interface InferenceMixin {
	/**
	 * Local-inference concurrency cap. How many parallel generate requests
	 * the hardware can handle simultaneously — matches the BatchScheduler's
	 * n_seq_max. TS admission control should read this at startup and cap
	 * its own slots to match.
	 */
	inferenceCapacity(): Promise<number>;
}

export function InferenceMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements InferenceMixin {
		async inferenceCapacity(): Promise<number> {
			const response = await this.request({ command: 'inference/capacity' });
			if (!response.success) {
				throw new Error(response.error || 'Failed to get inference capacity');
			}
			const capacity = Number((response.result as { capacity: number }).capacity);
			if (!Number.isFinite(capacity) || capacity < 1) {
				throw new Error(`Invalid inference capacity from Rust: ${capacity}`);
			}
			return capacity;
		}
	};
}
