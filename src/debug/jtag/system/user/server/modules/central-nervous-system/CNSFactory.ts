/**
 * CNSFactory - Creates PersonaCentralNervousSystem instances
 *
 * All scheduling is delegated to Rust. The factory wires the Rust bridge
 * and callbacks into the CNS config.
 */

// Note: Avoiding direct PersonaUser import to prevent circular dependency
import type { ModelCapabilities } from './CNSTypes';
import { CNSTier } from './CNSTypes';
import { PersonaCentralNervousSystem } from './PersonaCentralNervousSystem';
import type { PersonaInbox } from '../PersonaInbox';
import type { PersonaStateManager } from '../PersonaState';

// Import QueueItem type for handleChatMessageFromCNS signature
import type { QueueItem } from '../PersonaInbox';

// Import RustCognitionBridge type
import type { RustCognitionBridge } from '../RustCognitionBridge';

// Type for PersonaUser (avoid circular dependency)
interface PersonaUserLike {
  entity: {
    id: string;
    displayName?: string;
    uniqueId: string;
    modelConfig?: {
      capabilities?: readonly string[];
    };
  };
  inbox: PersonaInbox;
  prefrontal: {
    personaState: PersonaStateManager;
  } | null;
  // Rust cognition bridge (required for scheduling)
  rustCognitionBridge: RustCognitionBridge | null;
  handleChatMessageFromCNS: (item: QueueItem) => Promise<void>;
  pollTasksFromCNS: () => Promise<void>;
  generateSelfTasksFromCNS: () => Promise<void>;
}

export class CNSFactory {
  /**
   * Create CNS instance based on persona's capabilities
   */
  static create(persona: PersonaUserLike): PersonaCentralNervousSystem {
    const capabilities = this.parseCapabilities(persona.entity.modelConfig?.capabilities);
    const tier = this.selectTier(capabilities);

    // All tiers currently use the same Rust-delegated CNS
    // Future: tier could influence Rust scheduling parameters
    if (tier !== CNSTier.DETERMINISTIC) {
      console.warn(`CNS tier ${tier} not yet differentiated, using Rust-delegated scheduling for ${persona.entity.displayName || persona.entity.id}`);
    }

    return this.createRustDelegatedCNS(persona);
  }

  /**
   * Parse string[] capabilities from modelConfig into ModelCapabilities object
   */
  private static parseCapabilities(capabilitiesArray: readonly string[] | undefined): ModelCapabilities | undefined {
    if (!capabilitiesArray || capabilitiesArray.length === 0) {
      return undefined;
    }

    const capabilities: Partial<ModelCapabilities> = {};

    for (const cap of capabilitiesArray) {
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
   * Select CNS tier based on model capabilities
   */
  private static selectTier(capabilities: ModelCapabilities | undefined): CNSTier {
    if (!capabilities) {
      return CNSTier.DETERMINISTIC;
    }

    if (capabilities['advanced-reasoning'] && capabilities['meta-cognition']) {
      return CNSTier.NEURAL;
    }

    if (capabilities['moderate-reasoning'] && capabilities['pattern-recognition']) {
      return CNSTier.HEURISTIC;
    }

    return CNSTier.DETERMINISTIC;
  }

  /**
   * Create Rust-delegated CNS
   * All scheduling decisions made by Rust via IPC.
   */
  private static createRustDelegatedCNS(persona: PersonaUserLike): PersonaCentralNervousSystem {
    if (!persona.prefrontal) {
      throw new Error('CNSFactory.create() called before PrefrontalCortex initialized');
    }

    if (!persona.rustCognitionBridge) {
      throw new Error('CNSFactory.create() called without Rust cognition bridge — Rust bridge is required');
    }

    const personaName = persona.entity.displayName || 'Unknown';

    return new PersonaCentralNervousSystem({
      inbox: persona.inbox,
      personaState: persona.prefrontal.personaState,
      rustBridge: persona.rustCognitionBridge,
      personaId: persona.entity.id,
      personaName,
      uniqueId: persona.entity.uniqueId,
      handleChatMessage: async (item: QueueItem): Promise<void> => {
        await persona.handleChatMessageFromCNS(item);
      },
      pollTasks: async (): Promise<void> => {
        await persona.pollTasksFromCNS();
      },
      generateSelfTasks: async (): Promise<void> => {
        await persona.generateSelfTasksFromCNS();
      },
      allowBackgroundThreads: false,
    });
  }
}
