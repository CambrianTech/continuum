// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Identity System - Modular Architecture with Separation of Burden
 * 
 * ARCHITECTURAL INSIGHTS:
 * - UniversalIdentity: 90% shared foundation (~200 lines)
 * - Specialized Classes: 10% specific implementation (50-100 lines each)
 * - Differences cascade down to subclasses where they have relevance
 * - No god objects - each class has focused responsibility
 * 
 * MODULAR DESIGN:
 * - Foundation handles universal infrastructure
 * - Specialized classes handle specific behaviors
 * - Easy to extend with new identity types
 * - Clean separation of concerns
 * 
 * REFACTORED FROM GOD OBJECT PATTERN:
 * Previous approach had redundant inheritance chains and god objects.
 * New approach: Clean foundation with specialized implementations.
 */

// ==================== NEW MODULAR ARCHITECTURE ====================

// Foundation
export { UniversalIdentity } from './UniversalIdentity';
export type { BaseCapabilities, BaseMetadata, UniversalState, UniversalEvent } from './UniversalIdentity';

// Specialized Identity Classes with their types
export { HumanUser, createHumanUser } from './HumanUser';
export type { HumanMetadata, HumanCapabilities } from './HumanUser';
export { PersonaAgent, createPersonaAgent } from './PersonaAgent';
export type { PersonaMetadata, PersonaCapabilities } from './PersonaAgent';
export { SystemAgent, createSystemAgent } from './SystemAgent';
export type { SystemMetadata, SystemCapabilities } from './SystemAgent';

// Import for local use
import { UniversalIdentity } from './UniversalIdentity';
import { createHumanUser } from './HumanUser';
import { createPersonaAgent } from './PersonaAgent';
import { createSystemAgent } from './SystemAgent';
import type { BaseMetadata } from './UniversalIdentity';
import type { HumanMetadata } from './HumanUser';
import type { PersonaMetadata } from './PersonaAgent';
import type { SystemMetadata } from './SystemAgent';

// ==================== UNIVERSAL FACTORY CONFIG ====================

/**
 * Base configuration for all identity types
 */
export interface BaseIdentityConfig {
  id?: string;
  name: string;
  metadata?: Partial<BaseMetadata>;
}

/**
 * Extended configuration for human identities
 */
export interface HumanIdentityConfig extends BaseIdentityConfig {
  email?: string;
  metadata?: Partial<HumanMetadata>;
}

/**
 * Extended configuration for persona identities
 */
export interface PersonaIdentityConfig extends BaseIdentityConfig {
  prompt?: string;
  specialization?: string;
  metadata?: Partial<PersonaMetadata>;
}

/**
 * Extended configuration for system identities
 */
export interface SystemIdentityConfig extends BaseIdentityConfig {
  permissions?: string[];
  metadata?: Partial<SystemMetadata>;
}

// ==================== NEW UNIVERSAL FACTORY ====================

/**
 * Factory function for creating any identity type
 */
export function createIdentity(type: 'human', config: HumanIdentityConfig): UniversalIdentity;
export function createIdentity(type: 'persona', config: PersonaIdentityConfig): UniversalIdentity;
export function createIdentity(type: 'system', config: SystemIdentityConfig): UniversalIdentity;
export function createIdentity(type: 'human' | 'persona' | 'system', config: BaseIdentityConfig): UniversalIdentity {
  switch (type) {
    case 'human':
      return createHumanUser(config);
    case 'persona':
      return createPersonaAgent(config);
    case 'system':
      return createSystemAgent(config);
    default:
      throw new Error(`Unknown identity type: ${type}`);
  }
}

// ==================== LEGACY EXPORTS REMOVED ====================

// All legacy identity classes have been removed and replaced with:
// - UniversalIdentity foundation with generic types
// - HumanUser, PersonaAgent, SystemAgent specialized classes
// - Clean architecture with proper separation of concerns

/**
 * Identity type registry for runtime checks
 */
export const IDENTITY_TYPES = {
  HUMAN: 'human',
  PERSONA: 'persona',
  SYSTEM: 'system'
} as const;

export type IdentityType = typeof IDENTITY_TYPES[keyof typeof IDENTITY_TYPES];

// ==================== NEW ARCHITECTURE EXAMPLES ====================

/**
 * Example: Creating a human user with proper separation of concerns
 */
/*
import { createHumanUser } from './core/identity';

const user = createHumanUser({
  name: 'Joel',
  email: 'joel@example.com',
  metadata: {
    theme: 'dark',
    notifications: true
  }
});

// Human-specific capabilities automatically configured
await user.initialize();
await user.joinRoom('general');
await user.sendMessage('Hello everyone!', 'general');
*/

/**
 * Example: Creating a persona agent with Academy capabilities
 */
/*
import { createPersonaAgent } from './core/identity';

const persona = createPersonaAgent({
  name: 'TypeScript Expert',
  prompt: 'You are a TypeScript expert who helps fix compilation errors.',
  specialization: 'typescript'
});

// Persona-specific capabilities automatically configured
await persona.initialize();
await persona.joinRoom('help');
// Persona can learn from interactions and evolve
*/

/**
 * Example: Creating a system agent with admin capabilities
 */
/*
import { createSystemAgent } from './core/identity';

const systemAgent = createSystemAgent({
  name: 'System Monitor',
  permissions: ['admin', 'monitor', 'cleanup']
});

// System-specific capabilities automatically configured
await systemAgent.initialize();
// System can handle administrative tasks
*/

// ==================== ARCHITECTURAL BENEFITS ====================

/**
 * This demonstrates proper separation of burden:
 * 
 * - Universal foundation (UniversalIdentity) - 90% shared infrastructure
 * - Specialized implementations (HumanUser, PersonaAgent, SystemAgent) - 10% specific
 * - Differences cascade down to subclasses where they have relevance
 * - No god objects - each class has focused responsibility
 * 
 * Benefits:
 * - Code reuse through shared foundation
 * - Separation of concerns through specialized classes
 * - Modularity - easy to extend with new identity types
 * - Type safety through strong interfaces
 * - Clean architecture - no redundant inheritance chains
 */