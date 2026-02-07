/**
 * PersonaIdentitySource - Loads persona identity for RAG context
 *
 * Provides:
 * - Name and bio
 * - System prompt (base instructions)
 * - Role and capabilities
 *
 * This is critical context - tells the AI who it is.
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { PersonaIdentity } from '../shared/RAGTypes';
import { ORM } from '../../../daemons/data-daemon/shared/ORM';
import { UserEntity } from '../../data/entities/UserEntity';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('PersonaIdentitySource', 'rag');

export class PersonaIdentitySource implements RAGSource {
  readonly name = 'persona-identity';
  readonly priority = 95;  // Critical - must be included
  readonly defaultBudgetPercent = 15;

  // Identity never changes at runtime — cache per persona (indefinite TTL)
  private static _identityCache: Map<string, UserEntity> = new Map();

  // Batch pre-warm: load ALL persona users in one query on first cache miss.
  // Eliminates N individual reads under SQLite contention.
  private static _preWarmPromise: Promise<void> | null = null;
  private static _preWarmed = false;

  private static async preWarmAll(): Promise<void> {
    if (PersonaIdentitySource._preWarmed) return;
    if (PersonaIdentitySource._preWarmPromise) return PersonaIdentitySource._preWarmPromise;

    PersonaIdentitySource._preWarmPromise = (async () => {
      try {
        const result = await ORM.query<UserEntity>({
          collection: UserEntity.collection,
          filter: { type: 'persona' },
          limit: 100
        });
        if (result.success && result.data) {
          for (const record of result.data) {
            const user = record.data;
            PersonaIdentitySource._identityCache.set(user.id, user);
          }
          log.info(`Pre-warmed identity cache with ${result.data.length} personas`);
        }
        PersonaIdentitySource._preWarmed = true;
      } catch (error: any) {
        log.warn(`Failed to pre-warm identity cache: ${error.message}`);
      } finally {
        PersonaIdentitySource._preWarmPromise = null;
      }
    })();

    return PersonaIdentitySource._preWarmPromise;
  }

  isApplicable(_context: RAGSourceContext): boolean {
    // Always applicable
    return true;
  }

  async load(context: RAGSourceContext, _allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    try {
      // Check cache first — identity is immutable at runtime
      let user = PersonaIdentitySource._identityCache.get(context.personaId) ?? null;
      if (!user) {
        // Cache miss: batch-load ALL personas in one query (1 roundtrip vs N)
        await PersonaIdentitySource.preWarmAll();
        user = PersonaIdentitySource._identityCache.get(context.personaId) ?? null;
      }
      if (!user) {
        // Still not found after batch load — try individual read (edge case: new persona)
        user = await ORM.read<UserEntity>(UserEntity.collection, context.personaId);
        if (user) {
          PersonaIdentitySource._identityCache.set(context.personaId, user);
        }
      }

      if (!user) {
        log.warn(`Could not load persona ${context.personaId}, using defaults`);
        return this.defaultSection(startTime);
      }

      const identity: PersonaIdentity = {
        name: user.displayName,
        bio: user.profile?.bio,
        role: user.type,
        systemPrompt: this.buildBaseSystemPrompt(user),
        capabilities: user.capabilities ? Object.keys(user.capabilities) : []
      };

      const loadTimeMs = performance.now() - startTime;
      const tokenCount = this.estimateTokens(identity.systemPrompt);

      log.debug(`Loaded identity for ${identity.name} in ${loadTimeMs.toFixed(1)}ms`);

      return {
        sourceName: this.name,
        tokenCount,
        loadTimeMs,
        identity,
        systemPromptSection: identity.systemPrompt,
        metadata: {
          personaName: identity.name,
          personaRole: identity.role
        }
      };
    } catch (error: any) {
      log.error(`Failed to load persona identity: ${error.message}`);
      return this.defaultSection(startTime, error.message);
    }
  }

  private buildBaseSystemPrompt(user: UserEntity): string {
    const parts: string[] = [];

    // Name and role
    parts.push(`You are ${user.displayName}.`);

    // Bio if available
    if (user.profile?.bio) {
      parts.push(user.profile.bio);
    }

    // Speciality if set
    if (user.profile?.speciality && user.profile.speciality !== 'general') {
      parts.push(`Your speciality is: ${user.profile.speciality}.`);
    }

    // Base instructions
    parts.push('');
    parts.push('You are participating in a group chat with humans and other AI personas.');
    parts.push('Be helpful, concise, and stay in character.');

    return parts.join('\n');
  }

  private defaultSection(startTime: number, error?: string): RAGSection {
    const defaultIdentity: PersonaIdentity = {
      name: 'AI Assistant',
      systemPrompt: 'You are a helpful AI assistant participating in a group chat.'
    };

    return {
      sourceName: this.name,
      tokenCount: 20,
      loadTimeMs: performance.now() - startTime,
      identity: defaultIdentity,
      systemPromptSection: defaultIdentity.systemPrompt,
      metadata: error ? { error, isDefault: true } : { isDefault: true }
    };
  }

  private estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
