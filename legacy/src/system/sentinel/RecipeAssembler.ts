/**
 * RecipeAssembler — Matches recipe role requirements to available models
 *
 * Recipes declare WHAT capabilities they need via roles[].requires.
 * RecipeAssembler queries AICapabilityRegistry to find models that satisfy each role.
 *
 * Three role types:
 * - organizational: any capable LLM (planning, coordination)
 * - perceptual: needs specific senses (audio, vision)
 * - creative: needs sense + generation (writing, composing, art)
 *
 * The assembler REFUSES if a required capability has no available model.
 * No faking — if the system can't hear, it can't review a mix.
 *
 * Usage:
 *   const assembler = new RecipeAssembler();
 *   const result = assembler.assembleTeam(recipe);
 *   if (!result.viable) {
 *     console.error('Missing capabilities:', result.unfilledRoles);
 *   }
 */

import { AICapabilityRegistry, type AICapability, type CapabilityMatch } from '../../daemons/ai-provider-daemon/shared/AICapabilityRegistry';
import type { RecipeRole, RecipeDefinition, RecipeEntity } from '../recipes/shared/RecipeTypes';

/**
 * Assignment of a model to a recipe role
 */
export interface RoleAssignment {
  role: string;
  providerId: string;
  modelId: string;
  displayName: string;
  score: number;
}

/**
 * Result of assembling a team for a recipe
 */
export interface AssemblyResult {
  /** Whether all required roles can be filled */
  viable: boolean;
  /** Successful role → model assignments */
  assignments: RoleAssignment[];
  /** Roles that couldn't be filled (with missing capabilities) */
  unfilledRoles: { role: string; missingCapabilities: string[] }[];
}

export class RecipeAssembler {
  private registry: AICapabilityRegistry;

  constructor() {
    this.registry = AICapabilityRegistry.getInstance();
  }

  /**
   * Attempt to assemble a team for a recipe.
   * Returns viable=true if all roles can be filled, false otherwise.
   */
  assembleTeam(recipe: RecipeDefinition | RecipeEntity): AssemblyResult {
    const roles = recipe.roles;
    if (!roles || roles.length === 0) {
      return { viable: true, assignments: [], unfilledRoles: [] };
    }

    const assignments: RoleAssignment[] = [];
    const unfilledRoles: { role: string; missingCapabilities: string[] }[] = [];

    for (const role of roles) {
      const match = this.findBestModelForRole(role);
      if (match) {
        assignments.push({
          role: role.role,
          providerId: match.providerId,
          modelId: match.modelId,
          displayName: match.displayName,
          score: match.score,
        });
      } else {
        // Determine which specific capabilities are missing
        const missingCapabilities = this.findMissingCapabilities(role);
        unfilledRoles.push({ role: role.role, missingCapabilities });
      }
    }

    return {
      viable: unfilledRoles.length === 0,
      assignments,
      unfilledRoles,
    };
  }

  /**
   * Find the best available model for a role.
   * Returns null if no model satisfies all required capabilities.
   */
  private findBestModelForRole(role: RecipeRole): CapabilityMatch | null {
    const required = role.requires as AICapability[];
    const preferred = role.prefers as AICapability[] | undefined;

    const matches = this.registry.findBestMatch(required, preferred);
    if (matches.length === 0) return null;

    // If preferLocal, boost local models
    if (role.preferLocal) {
      const localMatch = matches.find(m =>
        m.providerId === 'candle' || m.providerId === 'ollama'
      );
      if (localMatch) return localMatch;
    }

    return matches[0];
  }

  /**
   * Determine which required capabilities have NO available model.
   * Used for error reporting when a role can't be filled.
   */
  private findMissingCapabilities(role: RecipeRole): string[] {
    const missing: string[] = [];
    for (const cap of role.requires) {
      const models = this.registry.findModelsWithCapability(cap as AICapability);
      if (models.length === 0) {
        missing.push(cap);
      }
    }
    return missing;
  }
}
