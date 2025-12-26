/**
 * CNSFactory - Capability-based factory for creating PersonaCentralNervousSystem instances
 *
 * Selects appropriate CNS tier (Deterministic/Heuristic/Neural) based on model capabilities,
 * NOT intelligence thresholds. This allows flexible configuration (e.g., high-intelligence
 * model doing simple status messages, or mid-tier model with full capabilities).
 */

/* eslint-disable @typescript-eslint/prefer-nullish-coalescing */
// Note: Using || instead of ?? because strictNullChecks is not enabled globally

// Note: Avoiding direct PersonaUser import to prevent circular dependency
// PersonaUser will import CNSFactory, so we use type-only reference
import type { ModelCapabilities } from './CNSTypes';
import { CNSTier } from './CNSTypes';
import { PersonaCentralNervousSystem } from './PersonaCentralNervousSystem';
import { ActivityDomain } from '../cognitive-schedulers/ICognitiveScheduler';
import { DeterministicCognitiveScheduler } from '../cognitive-schedulers/DeterministicCognitiveScheduler';
import type { PersonaInbox } from '../PersonaInbox';
import type { PersonaStateManager } from '../PersonaState';
import type { PersonaGenome } from '../PersonaGenome';
// Future imports:
// import { HeuristicCognitiveScheduler } from '../cognitive-schedulers/HeuristicCognitiveScheduler';
// import { NeuralCognitiveScheduler } from '../cognitive-schedulers/NeuralCognitiveScheduler';

// Type for PersonaUser (avoid circular dependency)
// Matches PersonaUser's interface for CNS creation
// Uses actual class types to ensure compile-time safety
interface PersonaUserLike {
  entity: {
    id: string;
    displayName?: string;  // UserEntity uses displayName, not name
    uniqueId: string;  // Format: {name}-{shortId} for log paths
    modelConfig?: {
      capabilities?: readonly string[];  // AI capabilities (e.g., ['advanced-reasoning'])
    };
  };
  homeDirectory: string;  // Persona's $HOME directory
  inbox: PersonaInbox;
  prefrontal: {
    personaState: PersonaStateManager;  // NEUROANATOMY: personaState in PrefrontalCortex
  } | null;  // Nullable during construction, but must be non-null when CNS is created
  memory: {
    genome: PersonaGenome;  // Phase 2: genome moved inside memory module
  };
  handleChatMessageFromCNS: (item: QueueItem) => Promise<void>;
  pollTasksFromCNS: () => Promise<void>;
  generateSelfTasksFromCNS: () => Promise<void>;
}

// Import QueueItem type for handleChatMessageFromCNS signature
import type { QueueItem } from '../PersonaInbox';

export class CNSFactory {
  /**
   * Create CNS instance with appropriate scheduler based on persona's capabilities
   */
  static create(persona: PersonaUserLike): PersonaCentralNervousSystem {
    // Map string[] capabilities from modelConfig to ModelCapabilities object
    const capabilities = this.parseCapabilities(persona.entity.modelConfig?.capabilities);
    const tier = this.selectTier(capabilities);

    switch (tier) {
      case CNSTier.DETERMINISTIC:
        return this.createDeterministicCNS(persona);

      case CNSTier.HEURISTIC:
        // TODO: Phase 2 - Implement HeuristicCognitiveScheduler
        console.warn(`⚠️ Heuristic CNS not yet implemented, falling back to deterministic for ${persona.entity.displayName || persona.entity.id}`);
        return this.createDeterministicCNS(persona);

      case CNSTier.NEURAL:
        // TODO: Phase 3 - Implement NeuralCognitiveScheduler
        console.warn(`⚠️ Neural CNS not yet implemented, falling back to deterministic for ${persona.entity.displayName || persona.entity.id}`);
        return this.createDeterministicCNS(persona);

      default:
        console.warn(`⚠️ Unknown CNS tier: ${tier}, falling back to deterministic for ${persona.entity.displayName || persona.entity.id}`);
        return this.createDeterministicCNS(persona);
    }
  }

  /**
   * Parse string[] capabilities from modelConfig into ModelCapabilities object
   * Maps UserEntity.modelConfig.capabilities (string[]) to CNS ModelCapabilities
   */
  private static parseCapabilities(capabilitiesArray: readonly string[] | undefined): ModelCapabilities | undefined {
    if (!capabilitiesArray || capabilitiesArray.length === 0) {
      return undefined;
    }

    // Build capabilities object with readonly properties
    const capabilities: Partial<ModelCapabilities> = {};

    for (const cap of capabilitiesArray) {
      // Map string capabilities to ModelCapabilities keys
      // Use type assertion since we're building the object
      const mutableCaps = capabilities as Record<string, boolean>;

      switch (cap) {
        case 'advanced-reasoning':
          mutableCaps['advanced-reasoning'] = true;
          break;
        case 'meta-cognition':
          mutableCaps['meta-cognition'] = true;
          break;
        case 'long-context':
          mutableCaps['long-context'] = true;
          break;
        case 'moderate-reasoning':
          mutableCaps['moderate-reasoning'] = true;
          break;
        case 'pattern-recognition':
          mutableCaps['pattern-recognition'] = true;
          break;
        case 'fast-inference':
          mutableCaps['fast-inference'] = true;
          break;
        case 'template-responses':
          mutableCaps['template-responses'] = true;
          break;
      }
    }

    return Object.keys(capabilities).length > 0 ? capabilities as ModelCapabilities : undefined;
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

    // Assert non-null: prefrontal must be initialized before CNS creation
    if (!persona.prefrontal) {
      throw new Error('CNSFactory.create() called before PrefrontalCortex initialized');
    }

    return new PersonaCentralNervousSystem({
      scheduler,
      inbox: persona.inbox,
      personaState: persona.prefrontal.personaState,  // NEUROANATOMY: personaState in PrefrontalCortex
      genome: persona.memory.genome,  // Phase 2: genome moved inside memory module
      personaId: persona.entity.id,
      personaName: persona.entity.displayName || 'Unknown',
      uniqueId: persona.entity.uniqueId,
      handleChatMessage: async (item: QueueItem): Promise<void> => {
        // Delegate to PersonaUser's existing chat handler
        await persona.handleChatMessageFromCNS(item);
      },
      pollTasks: async (): Promise<void> => {
        // Delegate to PersonaUser's task polling
        await persona.pollTasksFromCNS();
      },
      generateSelfTasks: async (): Promise<void> => {
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
