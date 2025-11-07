/**
 * CNSFactory - Capability-based factory for creating PersonaCentralNervousSystem instances
 *
 * Selects appropriate CNS tier (Deterministic/Heuristic/Neural) based on model capabilities,
 * NOT intelligence thresholds. This allows flexible configuration (e.g., high-intelligence
 * model doing simple status messages, or mid-tier model with full capabilities).
 */

// Note: Avoiding direct PersonaUser import to prevent circular dependency
// PersonaUser will import CNSFactory, so we use type-only reference
import type { ModelCapabilities } from './CNSTypes';
import { CNSTier } from './CNSTypes';
import { PersonaCentralNervousSystem } from './PersonaCentralNervousSystem';
import { ActivityDomain } from '../cognitive-schedulers/ICognitiveScheduler';
import { DeterministicCognitiveScheduler } from '../cognitive-schedulers/DeterministicCognitiveScheduler';
// Future imports:
// import { HeuristicCognitiveScheduler } from '../cognitive-schedulers/HeuristicCognitiveScheduler';
// import { NeuralCognitiveScheduler } from '../cognitive-schedulers/NeuralCognitiveScheduler';

// Type for PersonaUser (avoid circular dependency)
// Matches PersonaUser's interface for CNS creation
interface PersonaUserLike {
  entity: {
    id: string;
    displayName?: string;  // UserEntity uses displayName, not name
    capabilities?: any;  // UserCapabilities or Record<string, boolean>
  };
  inbox: any;
  personaState: any;
  genome: any;
  handleChatMessageFromCNS: (item: any) => Promise<void>;
  pollTasksFromCNS: () => Promise<void>;
  generateSelfTasksFromCNS: () => Promise<void>;
}

export class CNSFactory {
  /**
   * Create CNS instance with appropriate scheduler based on persona's capabilities
   */
  static create(persona: PersonaUserLike): PersonaCentralNervousSystem {
    const capabilities = persona.entity.capabilities as ModelCapabilities | undefined;
    const tier = this.selectTier(capabilities);

    switch (tier) {
      case CNSTier.DETERMINISTIC:
        return this.createDeterministicCNS(persona);

      case CNSTier.HEURISTIC:
        // TODO: Phase 2 - Implement HeuristicCognitiveScheduler
        console.warn(`Heuristic CNS not yet implemented, falling back to deterministic for ${persona.entity.displayName || persona.entity.id}`);
        return this.createDeterministicCNS(persona);

      case CNSTier.NEURAL:
        // TODO: Phase 3 - Implement NeuralCognitiveScheduler
        console.warn(`Neural CNS not yet implemented, falling back to deterministic for ${persona.entity.displayName || persona.entity.id}`);
        return this.createDeterministicCNS(persona);

      default:
        console.warn(`Unknown CNS tier: ${tier}, falling back to deterministic for ${persona.entity.displayName || persona.entity.id}`);
        return this.createDeterministicCNS(persona);
    }
  }

  /**
   * Select CNS tier based on model capabilities (NOT intelligence thresholds)
   */
  private static selectTier(capabilities: ModelCapabilities | undefined): CNSTier {
    if (!capabilities) {
      return CNSTier.DETERMINISTIC;
    }

    // Neural tier: Frontier models with advanced reasoning and meta-cognition
    if (capabilities['advanced-reasoning'] && capabilities['meta-cognition']) {
      return CNSTier.NEURAL;
    }

    // Heuristic tier: Mid-tier models with moderate reasoning and pattern recognition
    if (capabilities['moderate-reasoning'] && capabilities['pattern-recognition']) {
      return CNSTier.HEURISTIC;
    }

    // Deterministic tier: Simple models with fast inference or template responses
    return CNSTier.DETERMINISTIC;
  }

  /**
   * Create Deterministic CNS (Phase 1 - simplest scheduler)
   * Works with ANY model - no capability requirements
   */
  private static createDeterministicCNS(persona: PersonaUserLike): PersonaCentralNervousSystem {
    // DeterministicCognitiveScheduler takes no constructor arguments
    const scheduler = new DeterministicCognitiveScheduler();

    return new PersonaCentralNervousSystem({
      scheduler,
      inbox: persona.inbox,
      personaState: persona.personaState,
      genome: persona.genome,
      personaId: persona.entity.id,
      personaName: persona.entity.displayName || 'Unknown',
      handleChatMessage: async (item) => {
        // Delegate to PersonaUser's existing chat handler
        await persona.handleChatMessageFromCNS(item);
      },
      pollTasks: async () => {
        // Delegate to PersonaUser's task polling
        await persona.pollTasksFromCNS();
      },
      generateSelfTasks: async () => {
        // Delegate to PersonaUser's self-task generation
        await persona.generateSelfTasksFromCNS();
      },
      enabledDomains: [ActivityDomain.CHAT, ActivityDomain.BACKGROUND],
      allowBackgroundThreads: false  // Phase 1: Chat only
    });
  }

  // TODO: Phase 2 - Add createHeuristicCNS
  // TODO: Phase 3 - Add createNeuralCNS
}
