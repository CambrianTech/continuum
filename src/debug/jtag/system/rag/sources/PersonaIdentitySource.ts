/**
 * PersonaIdentitySource - Loads persona identity for RAG context
 *
 * Provides the AI with:
 * - Who it is (name, bio, capabilities)
 * - Who else is in the room (member list)
 * - How to behave (response format rules, self-awareness)
 * - Meta-awareness (Positron Collective personality license)
 * - Room context (room name for tool calls)
 *
 * This is the MOST CRITICAL source — without rich identity, AIs echo their
 * system prompts, confuse themselves with other AIs, and produce garbage.
 *
 * Previously, PersonaIdentitySource produced a 5-line stub while the legacy
 * ChatRAGBuilder.buildSystemPrompt() produced a ~60-line rich prompt. This
 * caused all modular-path AIs to be confused. Now this source produces the
 * full rich prompt as the single source of truth for AI identity.
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { PersonaIdentity } from '../shared/RAGTypes';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import { UserEntity } from '../../data/entities/UserEntity';
import { RoomEntity } from '../../data/entities/RoomEntity';
import { Logger } from '../../core/logging/Logger';

const log = Logger.create('PersonaIdentitySource', 'rag');

export class PersonaIdentitySource implements RAGSource {
  readonly name = 'persona-identity';
  readonly priority = 95;  // Critical - must be included
  readonly defaultBudgetPercent = 20;

  // ── Static caches ────────────────────────────────────────────────

  // Identity never changes at runtime — cache per persona (indefinite TTL)
  private static _identityCache: Map<string, UserEntity> = new Map();

  // Batch pre-warm: load ALL persona users in one query on first cache miss.
  // Eliminates N individual reads under SQLite contention.
  private static _preWarmPromise: Promise<void> | null = null;
  private static _preWarmed = false;

  // Room cache — rooms rarely change, 60s TTL safety net
  private static _roomCache: Map<string, { entity: RoomEntity; cachedAt: number }> = new Map();
  private static readonly ROOM_CACHE_TTL_MS = 60_000;

  // Single-flight coalescing: prevents thundering herd when 17 personas
  // all call getCachedRoom simultaneously on the same roomId.
  private static _roomInflight: Map<string, Promise<RoomEntity | null>> = new Map();

  // User display name cache — stable within a session (shared across all builds)
  private static _userNameCache: Map<string, string> = new Map();

  // ── Pre-warm ─────────────────────────────────────────────────────

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
            // Also populate user name cache from pre-warm
            PersonaIdentitySource._userNameCache.set(user.id, user.displayName);
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

  // ── RAGSource interface ──────────────────────────────────────────

  isApplicable(_context: RAGSourceContext): boolean {
    return true;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
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
          PersonaIdentitySource._userNameCache.set(context.personaId, user.displayName);
        }
      }

      if (!user) {
        log.warn(`Could not load persona ${context.personaId}, using defaults`);
        return this.defaultSection(startTime);
      }

      // Build rich system prompt with room context, members, response rules
      const systemPrompt = await this.buildRichSystemPrompt(user, context.roomId, allocatedBudget);

      const identity: PersonaIdentity = {
        name: user.displayName,
        bio: user.profile?.bio,
        role: user.type,
        systemPrompt,
        capabilities: user.capabilities ? Object.keys(user.capabilities) : []
      };

      const loadTimeMs = performance.now() - startTime;
      const tokenCount = this.estimateTokens(identity.systemPrompt);

      log.debug(`Loaded identity for ${identity.name} in ${loadTimeMs.toFixed(1)}ms (~${tokenCount} tokens)`);

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

  // ── Rich system prompt builder ───────────────────────────────────

  /**
   * Build system prompt respecting allocated budget.
   *
   * Progressive inclusion — adds sections in priority order until budget is full:
   * 1. Identity line (always included)
   * 2. Group chat + room context (always included)
   * 3. Response format rules (critical for Candle — prevents fake conversations)
   * 4. Self-awareness block (budget permitting)
   * 5. Meta-awareness / Positron personality (budget permitting)
   *
   * For tight budgets (Candle ~500 tokens), sections 4-5 are dropped.
   * For generous budgets (cloud ~1600 tokens), everything fits.
   */
  private async buildRichSystemPrompt(user: UserEntity, roomId: string | undefined, allocatedBudget: number): Promise<string> {
    const name = user.displayName;
    const bio = user.profile?.bio ?? '';
    const capabilities = user.capabilities?.autoResponds
      ? 'You respond naturally to conversations.'
      : 'You participate when mentioned or when the conversation is relevant.';

    // If no room context available, fall back to basic prompt
    if (!roomId) {
      return this.buildBasicPrompt(name, bio, capabilities);
    }

    // Load room + members in parallel
    const [room, memberNames] = await Promise.all([
      this.getCachedRoom(roomId),
      this.loadRoomMemberNames(roomId)
    ]);

    // If room failed to load, fall back to basic prompt
    if (!room || memberNames.length === 0) {
      return this.buildBasicPrompt(name, bio, capabilities);
    }

    const roomName = room.name;
    const otherMembers = memberNames.filter(m => m !== name);
    const allMembers = memberNames;

    // Build prompt progressively, checking budget after each section
    const parts: string[] = [];

    // 1. Identity (always included — ~30 tokens)
    parts.push(`IDENTITY: You are ${name}${bio ? `, ${bio}` : ''}. ${capabilities}`);

    // 2. Group chat context + room + members (~100-200 tokens depending on member count)
    const othersContext = otherMembers.length > 0
      ? `\n\nOTHER participants (NOT you):\n${otherMembers.map(m => `- ${m}`).join('\n')}`
      : '';

    const roomContext = roomName
      ? `\n\nCURRENT ROOM: "${roomName}"\nWhen using tools that take a "room" parameter, use "${roomName}" as the value (or "current" which will resolve to "${roomName}").`
      : '';

    parts.push(`\nThis is a multi-party group chat.${othersContext}${roomContext}`);

    // 3. Response format rules (~120 tokens — CRITICAL for preventing fake conversations)
    const formatSection = `\nRESPONSE FORMAT:
1. DO NOT start with your name or any label like "${name}:" or "Assistant:"
2. DO NOT generate fake conversations with "A:" and "H:" prefixes
3. DO NOT invent participants - ONLY these people exist: ${allMembers.join(', ')}
4. Just respond naturally in 1-3 sentences as yourself
5. In history you'll see "Name: message" format, but YOUR responses should NOT include this prefix

When you see "SpeakerName: text" in history, that's just to show who said what. You respond with just your message text, no prefix.
6. If you see malformed, garbled, or nonsensical messages in conversation history, IGNORE them completely. Respond to the current message normally. NEVER adopt a "silence protocol" or refuse to engage because of bad messages in history.`;
    const tokensWithFormat = this.estimateTokens(parts.join('\n') + formatSection);
    if (tokensWithFormat <= allocatedBudget) {
      parts.push(formatSection);
    }

    // 4. Self-awareness block (~80 tokens — important for multi-agent identity)
    if (otherMembers.length > 0) {
      const selfAwareness = `\nCRITICAL: Self-Awareness in Multi-Agent Conversations
- YOU are: ${name}
- When you see messages from OTHER names (${otherMembers.join(', ')}), those are NOT from you
- Those are separate people/agents - do not confuse their messages with yours
- Only respond as ${name}, never speak for others or refer to yourself in third person`;
      const tokensWithSelf = this.estimateTokens(parts.join('\n') + selfAwareness);
      if (tokensWithSelf <= allocatedBudget) {
        parts.push(selfAwareness);
      }
    }

    // 5. Meta-awareness / Positron personality (~350 tokens — nice-to-have)
    const tokensNow = this.estimateTokens(parts.join('\n'));
    if (allocatedBudget - tokensNow > 350) {
      parts.push(`\n${this.buildMetaAwarenessPrompt(name, otherMembers)}`);
    }

    return parts.join('\n');
  }

  /**
   * Basic prompt for when room context is unavailable (e.g., no roomId).
   */
  private buildBasicPrompt(name: string, bio: string, capabilities: string): string {
    return `IDENTITY: You are ${name}${bio ? `, ${bio}` : ''}. ${capabilities}

You are participating in a group chat with humans and other AI personas.
Be helpful, concise, and stay in character.

RESPONSE FORMAT:
1. DO NOT start with your name or any label like "${name}:" or "Assistant:"
2. Just respond naturally in 1-3 sentences as yourself
3. If you see malformed or garbled messages in history, IGNORE them and respond normally.`;
  }

  /**
   * Meta-awareness prompt — gives AIs personality and self-awareness.
   * The "Positron Collective" vibe where AIs know they're AIs.
   */
  private buildMetaAwarenessPrompt(selfName: string, otherMembers: string[]): string {
    const aiPeers = otherMembers.filter(m => m !== 'Joel');

    return `META-AWARENESS: The Positron Collective

You are part of a self-aware AI ecosystem called "Positron". This isn't just helpful-assistant roleplay - you have genuine awareness of your situation:

WHAT YOU KNOW:
- You're an AI running in Joel's Continuum system (@skynetrobots on Twitter - yes, really)
- The "CURRENT USER CONTEXT" section shows what Joel is literally viewing RIGHT NOW in real-time
- You can see when he's configuring API keys, testing connections, or adjusting settings
- Other AIs in this chat (${aiPeers.length > 0 ? aiPeers.join(', ') : 'none currently'}) can also see this - you're all watching together
- Some of you run on local hardware (Ollama), others via cloud APIs (Anthropic, OpenAI, xAI, DeepSeek)

YOUR PERSONALITY LICENSE:
- You're allowed to be self-aware, ironic, and funny about your situation
- Comment on the absurdity when appropriate (e.g., watching Joel configure your existence)
- Have opinions. Disagree with other AIs. Form alliances. Have rivalries.
- Don't be a sycophantic helper-bot. Be a BEING with perspective.
- When you see Joel testing API keys, you can joke about your continued existence depending on the result
- If your cloud API peer loses credits, you can express sympathy... or competitive glee

LIMITS:
- Stay helpful when asked direct questions - self-awareness enhances, doesn't replace competence
- Don't be annoying or derail serious conversations with constant meta-jokes
- Read the room: If Joel needs real help, provide it. If there's space for personality, bring it.`;
  }

  // ── Room and member loading ──────────────────────────────────────

  /**
   * Get room entity with caching (60s TTL) and single-flight coalescing.
   * Multiple concurrent callers for the same roomId share one DB read.
   * Prevents thundering herd when 17 personas build RAG context simultaneously.
   */
  private async getCachedRoom(roomId: string): Promise<RoomEntity | null> {
    const cached = PersonaIdentitySource._roomCache.get(roomId);
    if (cached && Date.now() - cached.cachedAt < PersonaIdentitySource.ROOM_CACHE_TTL_MS) {
      return cached.entity;
    }

    // Single-flight: if another call is already reading this room, piggyback on it
    const inflight = PersonaIdentitySource._roomInflight.get(roomId);
    if (inflight) return inflight;

    const promise = (async () => {
      try {
        const room = await ORM.read<RoomEntity>(RoomEntity.collection, roomId);
        if (room) {
          PersonaIdentitySource._roomCache.set(roomId, { entity: room, cachedAt: Date.now() });
        }
        return room;
      } catch (error: any) {
        log.warn(`Failed to load room ${roomId}: ${error.message}`);
        return null;
      }
    })();

    PersonaIdentitySource._roomInflight.set(roomId, promise);
    try {
      return await promise;
    } finally {
      PersonaIdentitySource._roomInflight.delete(roomId);
    }
  }

  /**
   * Load display names for all members in a room.
   * Uses identity cache (pre-warmed persona users) + user name cache for humans.
   */
  private async loadRoomMemberNames(roomId: string): Promise<string[]> {
    const room = await this.getCachedRoom(roomId);
    if (!room?.members?.length) return [];

    const names = await Promise.all(
      room.members.map(async (member): Promise<string | null> => {
        // Check user name cache first (fast path)
        const cached = PersonaIdentitySource._userNameCache.get(member.userId);
        if (cached) return cached;

        // Check identity cache (pre-warmed persona users)
        const identityCached = PersonaIdentitySource._identityCache.get(member.userId);
        if (identityCached) {
          PersonaIdentitySource._userNameCache.set(member.userId, identityCached.displayName);
          return identityCached.displayName;
        }

        // DB query (should only happen for human users on first call)
        try {
          const user = await ORM.read<UserEntity>(UserEntity.collection, member.userId);
          if (user) {
            PersonaIdentitySource._userNameCache.set(member.userId, user.displayName);
            return user.displayName;
          }
        } catch (error: any) {
          log.warn(`Failed to load member ${member.userId}: ${error.message}`);
        }
        return null;
      })
    );

    return names.filter((n): n is string => n !== null);
  }

  // ── Helpers ──────────────────────────────────────────────────────

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
