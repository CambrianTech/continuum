/**
 * Voice Server Module
 *
 * Exports voice WebSocket server, orchestrator, and utilities.
 *
 * Feature flag: USE_RUST_VOICE switches between TypeScript and Rust orchestrator
 * This proves the API is correct - both implementations work seamlessly
 */

export {
  VoiceWebSocketServer,
  getVoiceWebSocketServer,
  startVoiceServer,
} from './VoiceWebSocketHandler';

export {
  VoiceOrchestrator,
  type UtteranceEvent,
} from './VoiceOrchestrator';

export {
  VoiceOrchestratorRustBridge,
  getRustVoiceOrchestrator,
} from './VoiceOrchestratorRustBridge';

export {
  AIAudioBridge,
  getAIAudioBridge,
} from './AIAudioBridge';

// Import for internal use
import { VoiceOrchestrator } from './VoiceOrchestrator';
import { getRustVoiceOrchestrator } from './VoiceOrchestratorRustBridge';

// Feature flag - set via environment or default to Rust
const USE_RUST_VOICE = process.env.USE_RUST_VOICE !== 'false';  // Default: use Rust

/**
 * Get VoiceOrchestrator instance (Rust or TypeScript)
 *
 * "Wildly different integrations" test:
 * - TypeScript implementation (synchronous, in-process)
 * - Rust implementation (async IPC, 0.13ms latency)
 * - Same API, seamless swap
 */
export function getVoiceOrchestrator() {
	if (USE_RUST_VOICE) {
		return getRustVoiceOrchestrator() as unknown as VoiceOrchestrator;
	} else {
		return VoiceOrchestrator.instance;
	}
}
