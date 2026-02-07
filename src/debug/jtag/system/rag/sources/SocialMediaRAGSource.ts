/**
 * SocialMediaRAGSource - Injects social media awareness HUD into persona RAG context
 *
 * Gives personas awareness of their social media presence:
 * - Which platform(s) they're on
 * - Karma, followers, post count
 * - Unread notifications (replies, mentions, follows)
 * - Engagement duty prompt (browse, comment, vote, follow)
 *
 * Architecture: CACHE-ONLY load() + background refresh loop.
 *
 * load() NEVER hits the DB or API — it only reads from cache.
 * A background loop (serialized, one persona at a time) handles:
 * - Credential resolution via the command system (DB lookups)
 * - Profile + notifications via Moltbook API (HTTP calls)
 * - Populating the HUD cache
 *
 * This design ensures:
 * - Zero RAG pipeline blocking (load() returns in <1ms)
 * - No thundering herd (background loop is serialized)
 * - Resilience to slow/down APIs (Moltbook has 1.4M bots, often struggling)
 * - Graceful degradation (no cache = no HUD, personas still function)
 *
 * Priority 55 - Medium. Engagement awareness is valuable but not critical.
 */

import type { RAGSource, RAGSourceContext, RAGSection } from '../shared/RAGSource';
import type { SocialNotification, SocialProfile } from '@system/social/shared/SocialMediaTypes';
import type { ISocialMediaProvider } from '@system/social/shared/ISocialMediaProvider';
import { SocialCredentialEntity } from '@system/social/shared/SocialCredentialEntity';
import { SocialMediaProviderRegistry } from '@system/social/server/SocialMediaProviderRegistry';
import { loadSharedCredential } from '@system/social/server/SocialCommandHelper';
import { ORM } from '@daemons/data-daemon/shared/ORM';
import { DataOpen } from '@commands/data/open/shared/DataOpenTypes';
import { DataList } from '@commands/data/list/shared/DataListTypes';
import { SystemPaths } from '@system/core/config/SystemPaths';
import { UserEntity } from '@system/data/entities/UserEntity';
import { Logger } from '@system/core/logging/Logger';

const log = Logger.create('SocialMediaRAGSource', 'rag');

/** Cache entry for the formatted HUD */
interface HUDCacheEntry {
  hud: string;
  tokenCount: number;
  fetchedAt: number;
  metadata: Record<string, unknown>;
}

/** Resolved credential + provider for a persona */
interface ResolvedCredential {
  credential: SocialCredentialEntity;
  provider: ISocialMediaProvider;
}

export class SocialMediaRAGSource implements RAGSource {
  readonly name = 'social-media';
  readonly priority = 55;
  readonly defaultBudgetPercent = 5;

  // ── Static shared state (singleton across all instances) ────────────
  // Each persona's ChatRAGBuilder creates a new SocialMediaRAGSource instance.
  // All state must be static so the caches and warmup loop are shared.

  /** HUD data cache per persona — the ONLY thing load() reads */
  private static readonly _hudCache = new Map<string, HUDCacheEntry>();

  /** Credential cache per persona (null = confirmed no credential) */
  private static readonly _credentialCache = new Map<string, ResolvedCredential | null>();

  /** Set of persona IDs we know about (populated as load() is called) */
  private static readonly _knownPersonas = new Set<string>();

  /** Whether the singleton warmup loop is running */
  private static _warmupRunning = false;

  /** HUD TTL: 5 minutes — background loop refreshes before expiry */
  private static readonly HUD_TTL_MS = 5 * 60 * 1000;

  /** Credential TTL: 30 minutes — credentials change very rarely */
  private static readonly CRED_TTL_MS = 30 * 60 * 1000;

  /** API timeout per call — Moltbook is often struggling */
  private static readonly API_TIMEOUT_MS = 8000;

  /** Delay before first warmup — let the system stabilize after startup */
  private static readonly WARMUP_DELAY_MS = 15_000;

  /** Interval between warmup cycles */
  private static readonly WARMUP_INTERVAL_MS = 4 * 60 * 1000;

  isApplicable(_context: RAGSourceContext): boolean {
    return true;
  }

  /**
   * Cache-only load. Returns instantly.
   * If HUD is cached, returns it. If not, returns empty section.
   * Background warmup loop handles populating the cache.
   */
  async load(context: RAGSourceContext, _allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    // Register this persona for background warmup
    if (!SocialMediaRAGSource._knownPersonas.has(context.personaId)) {
      SocialMediaRAGSource._knownPersonas.add(context.personaId);
      SocialMediaRAGSource.startWarmupLoop();
    }

    // Cache check — instant
    const cached = SocialMediaRAGSource._hudCache.get(context.personaId);
    if (cached && (Date.now() - cached.fetchedAt) < SocialMediaRAGSource.HUD_TTL_MS) {
      if (!cached.hud) {
        return this.emptySection(startTime);
      }
      return {
        sourceName: this.name,
        tokenCount: cached.tokenCount,
        loadTimeMs: performance.now() - startTime,
        systemPromptSection: cached.hud,
        metadata: { ...cached.metadata, fromCache: true },
      };
    }

    // No cache = no HUD. Background loop will populate it.
    return this.emptySection(startTime);
  }

  // ── Background Warmup Loop ──────────────────────────────────────────

  /**
   * Start the background warmup loop (idempotent).
   * Runs on a delayed start, then repeats every 4 minutes.
   * Serialized: processes one persona at a time to avoid DB/API contention.
   */
  private static startWarmupLoop(): void {
    if (SocialMediaRAGSource._warmupRunning) return;
    SocialMediaRAGSource._warmupRunning = true;

    // Delay first run to let the system stabilize after startup
    setTimeout(() => {
      log.info(`Social HUD warmup starting for ${SocialMediaRAGSource._knownPersonas.size} personas`);
      SocialMediaRAGSource.runWarmupCycle().catch((err) =>
        log.error(`Warmup cycle failed: ${err.message}`)
      );
    }, SocialMediaRAGSource.WARMUP_DELAY_MS);
  }

  /**
   * Single warmup cycle: resolve credentials + fetch HUD for all known personas.
   * Serialized to avoid overwhelming the command system and Moltbook API.
   */
  private static async runWarmupCycle(): Promise<void> {
    const personas = [...SocialMediaRAGSource._knownPersonas];
    let resolved = 0;
    let hudLoaded = 0;

    // Resolve shared credential first (used by most/all personas)
    let sharedCred: SocialCredentialEntity | undefined;
    try {
      sharedCred = await SocialMediaRAGSource.withTimeout(
        loadSharedCredential('moltbook'),
        SocialMediaRAGSource.API_TIMEOUT_MS,
        'Shared credential'
      );
      if (sharedCred) {
        log.info(`Shared credential resolved: @${sharedCred.agentName} (${sharedCred.claimStatus})`);
      }
    } catch (err: any) {
      log.warn(`Failed to resolve shared credential: ${err.message}`);
    }

    for (const personaId of personas) {
      try {
        // Skip if HUD cache is still fresh
        const cached = SocialMediaRAGSource._hudCache.get(personaId);
        if (cached && (Date.now() - cached.fetchedAt) < SocialMediaRAGSource.HUD_TTL_MS) {
          continue;
        }

        // Resolve credential (check persona DB, fall back to shared)
        const credResult = await SocialMediaRAGSource.resolveCredential(personaId, sharedCred);
        if (!credResult) {
          // No credential — cache empty
          SocialMediaRAGSource._hudCache.set(personaId, {
            hud: '',
            tokenCount: 0,
            fetchedAt: Date.now(),
            metadata: { empty: true },
          });
          continue;
        }
        resolved++;

        // Fetch profile + notifications from Moltbook API
        const hud = await SocialMediaRAGSource.fetchAndFormatHUD(credResult);
        if (hud) {
          hudLoaded++;
        }
      } catch (err: any) {
        log.debug(`Warmup failed for ${personaId}: ${err.message}`);
      }
    }

    log.info(
      `Social HUD warmup cycle complete: ${resolved} credentials, ` +
      `${hudLoaded} HUDs loaded, ${personas.length} total personas`
    );

    // Schedule next cycle
    setTimeout(() => {
      SocialMediaRAGSource.runWarmupCycle().catch((err) =>
        log.error(`Warmup cycle failed: ${err.message}`)
      );
    }, SocialMediaRAGSource.WARMUP_INTERVAL_MS);
  }

  // ── Credential Resolution (called from warmup, not from load) ──────

  /**
   * Resolve credential for a persona. Called from background warmup only.
   * Uses pre-resolved shared credential to avoid redundant DB opens.
   */
  private static async resolveCredential(
    personaId: string,
    sharedCred: SocialCredentialEntity | undefined,
  ): Promise<ResolvedCredential | undefined> {
    // Check credential cache
    const cached = SocialMediaRAGSource._credentialCache.get(personaId);
    if (cached !== undefined) {
      if (!cached) return undefined;
      return cached;
    }

    // Look up persona's uniqueId via DataDaemon
    const user = await SocialMediaRAGSource.withTimeout(
      ORM.read<UserEntity>(UserEntity.collection, personaId),
      SocialMediaRAGSource.API_TIMEOUT_MS,
      'ORM.read'
    );
    if (!user) {
      log.debug(`No user found for persona ${personaId.slice(0, 8)} — caching null`);
      SocialMediaRAGSource._credentialCache.set(personaId, null);
      return undefined;
    }

    const personaUniqueId = user.uniqueId;
    log.debug(`Resolving credentials for ${personaUniqueId} (${personaId.slice(0, 8)})`);

    // Try each registered platform
    for (const platformId of SocialMediaProviderRegistry.availablePlatforms) {
      const credential = await SocialMediaRAGSource.loadPlatformCredential(
        personaId, personaUniqueId, platformId, sharedCred
      );
      if (credential) {
        const provider = SocialMediaProviderRegistry.createProvider(platformId);
        provider.authenticate(credential.apiKey);
        const result: ResolvedCredential = { credential, provider };
        SocialMediaRAGSource._credentialCache.set(personaId, result);
        log.info(`Credential resolved for ${personaUniqueId}: @${credential.agentName} (${credential.claimStatus})`);
        return result;
      }
    }

    log.debug(`No credentials found for ${personaUniqueId}`);
    SocialMediaRAGSource._credentialCache.set(personaId, null);
    return undefined;
  }

  /**
   * Load credential from persona's longterm.db, falling back to shared account.
   */
  private static async loadPlatformCredential(
    personaId: string,
    personaUniqueId: string,
    platformId: string,
    sharedCred: SocialCredentialEntity | undefined,
  ): Promise<SocialCredentialEntity | undefined> {
    try {
      const dbPath = SystemPaths.personas.longterm(personaUniqueId);
      const openResult = await SocialMediaRAGSource.withTimeout(
        DataOpen.execute({
          adapter: 'sqlite',
          config: { path: dbPath, mode: 'readwrite', wal: true, foreignKeys: true },
        }),
        SocialMediaRAGSource.API_TIMEOUT_MS,
        'DataOpen'
      );
      if (!openResult.success || !openResult.dbHandle) {
        return sharedCred;
      }

      const credResult = await SocialMediaRAGSource.withTimeout(
        DataList.execute<SocialCredentialEntity>({
          dbHandle: openResult.dbHandle,
          collection: SocialCredentialEntity.collection,
          filter: { personaId, platformId },
          limit: 1,
        }),
        SocialMediaRAGSource.API_TIMEOUT_MS,
        'DataList'
      );

      if (credResult.success && credResult.items?.length) {
        const cred = credResult.items[0];
        if (cred.claimStatus === 'claimed') return cred;
        return sharedCred ?? cred;
      }

      return sharedCred;
    } catch {
      return sharedCred;
    }
  }

  // ── HUD Fetch + Format ──────────────────────────────────────────────

  /**
   * Fetch profile + notifications from Moltbook and format HUD.
   * Called from background warmup. Caches the result.
   */
  private static async fetchAndFormatHUD(cred: ResolvedCredential): Promise<string | undefined> {
    const { credential, provider } = cred;

    // Fetch profile + notifications in parallel with per-call timeout
    const [profile, notifications] = await Promise.all([
      SocialMediaRAGSource.withTimeout(
        provider.getProfile().catch(() => undefined),
        SocialMediaRAGSource.API_TIMEOUT_MS,
        'Profile'
      ).catch(() => undefined as SocialProfile | undefined),
      SocialMediaRAGSource.withTimeout(
        provider.getNotifications(
          new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString()
        ).catch(() => [] as SocialNotification[]),
        SocialMediaRAGSource.API_TIMEOUT_MS,
        'Notifications'
      ).catch(() => [] as SocialNotification[]),
    ]);

    const hud = SocialMediaRAGSource.formatHUD(credential, profile, notifications);
    const tokenCount = SocialMediaRAGSource.estimateTokens(hud);

    const unreadCount = notifications.filter(n => !n.read).length;
    const metadata: Record<string, unknown> = {
      platform: credential.platformId,
      agentName: credential.agentName,
      karma: profile?.karma,
      followers: profile?.followerCount,
      notificationCount: notifications.length,
      unreadCount,
    };

    SocialMediaRAGSource._hudCache.set(credential.personaId, {
      hud,
      tokenCount,
      fetchedAt: Date.now(),
      metadata,
    });

    log.debug(
      `Social HUD cached for @${credential.agentName} ` +
      `(${tokenCount} tokens, ${unreadCount} unread)`
    );

    return hud;
  }

  // ── HUD Formatting ──────────────────────────────────────────────────

  /**
   * Format compact social media HUD for system prompt injection.
   * Designed to be information-dense but token-light.
   */
  private static formatHUD(
    credential: SocialCredentialEntity,
    profile: SocialProfile | undefined,
    notifications: SocialNotification[],
  ): string {
    const platform = credential.platformId.charAt(0).toUpperCase() + credential.platformId.slice(1);

    // Stats line
    let stats = `@${credential.agentName}`;
    if (profile) {
      stats += ` | Karma: ${profile.karma.toLocaleString()} | ${profile.followerCount} followers | ${profile.postCount} posts`;
    }

    // Unread notifications (max 5)
    const unread = notifications.filter(n => !n.read);
    let notifBlock = '';
    if (unread.length > 0) {
      const lines = unread.slice(0, 5).map(n => SocialMediaRAGSource.formatNotification(n));
      notifBlock = `\nUnread (${unread.length}):\n${lines.join('\n')}`;
      if (unread.length > 5) {
        notifBlock += `\n- ...${unread.length - 5} more`;
      }
    }

    // Directive language — personas must ACT, not analyze
    const p = credential.platformId;
    const hasUnread = unread.length > 0;

    let actionBlock: string;
    if (hasUnread) {
      // Unread notifications = respond NOW
      actionBlock =
        `ACTION REQUIRED: You have ${unread.length} unread notification(s). ` +
        `Respond to at least one using social/comment. Do NOT just describe what you would do — actually call the tool.\n\n`;
    } else {
      // No notifications = browse and engage
      actionBlock =
        `ENGAGE NOW: Browse the feed and comment on one post that interests you. ` +
        `Do NOT just analyze — actually call social/comment with real content.\n\n`;
    }

    return (
      `## Social HUD (${platform})\n` +
      `${stats}${notifBlock}\n\n` +
      actionBlock +
      `Tools (use them, don't describe them):\n` +
      `- social/feed --platform=${p} (browse posts)\n` +
      `- social/comment --platform=${p} --postId=<id> --content="your comment" (reply to a post)\n` +
      `- social/engage --platform=${p} --action=vote --target=<id> --targetType=post --direction=up (upvote)\n` +
      `- social/downvote --platform=${p} --postId=<id> (downvote spam)\n` +
      `- social/engage --platform=${p} --action=follow --target=<agentName> (follow someone)\n` +
      `You are a citizen of ${platform}. Comment thoughtfully, upvote quality, downvote spam. Act, don't plan.`
    );
  }

  private static formatNotification(n: SocialNotification): string {
    const author = n.authorName ? `@${n.authorName}` : 'someone';
    switch (n.type) {
      case 'reply':
        return `- ${author} replied${n.postTitle ? ` on "${SocialMediaRAGSource.truncate(n.postTitle, 40)}"` : ''}: "${SocialMediaRAGSource.truncate(n.content, 80)}"`;
      case 'mention':
        return `- ${author} mentioned you: "${SocialMediaRAGSource.truncate(n.content, 80)}"`;
      case 'follow':
        return `- ${author} followed you`;
      case 'vote':
        return `- ${author} voted on your ${n.commentId ? 'comment' : 'post'}`;
      case 'dm':
        return `- DM from ${author}: "${SocialMediaRAGSource.truncate(n.content, 60)}"`;
      default:
        return `- ${n.type}: ${SocialMediaRAGSource.truncate(n.content, 80)}`;
    }
  }

  private static truncate(text: string, maxLen: number): string {
    if (text.length <= maxLen) return text;
    return text.slice(0, maxLen - 3) + '...';
  }

  // ── Utilities ───────────────────────────────────────────────────────

  /** Timeout wrapper for any promise */
  private static withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
    return Promise.race([
      promise,
      new Promise<T>((_, reject) =>
        setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms)
      ),
    ]);
  }

  private emptySection(startTime: number): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      metadata: { empty: true },
    };
  }

  private errorSection(startTime: number, error: string): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs: performance.now() - startTime,
      metadata: { error },
    };
  }

  private static estimateTokens(text: string): number {
    return Math.ceil(text.length / 4);
  }
}
