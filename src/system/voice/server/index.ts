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

// CRITICAL: Always instantiate the TS VoiceOrchestrator for its event listeners.
// Even when Rust handles utterance routing, the TS VoiceOrchestrator must:
// 1. Subscribe to 'persona:response:generated' → route AI responses to TTS via AIAudioBridge
// 2. Subscribe to 'voice:ai:speech' → log AI speech events
// Without this, AI voice responses are generated but never spoken.
let _tsOrchestratorInitialized = false;
function ensureTSVoiceOrchestrator(): void {
	if (!_tsOrchestratorInitialized) {
		_tsOrchestratorInitialized = true;
		// Access the singleton to trigger constructor + setupEventListeners()
		VoiceOrchestrator.instance;
	}
}

/**
 * Get VoiceOrchestrator instance (Rust or TypeScript)
 *
 * When USE_RUST_VOICE=true: Rust handles utterance routing (fast IPC),
 * but TS VoiceOrchestrator is always alive for persona:response:generated handling.
 */
export function getVoiceOrchestrator() {
	if (USE_RUST_VOICE) {
		ensureTSVoiceOrchestrator();
		return getRustVoiceOrchestrator() as unknown as VoiceOrchestrator;
	} else {
		return VoiceOrchestrator.instance;
	}
}
