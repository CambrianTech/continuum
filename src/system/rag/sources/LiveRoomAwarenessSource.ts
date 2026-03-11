/**
 * LiveRoomAwarenessSource — RAGSource for live room visual + participant awareness
 *
 * Two tiers of awareness:
 * 1. Participant metadata (free): who's in the call, mic/camera/screen state
 * 2. Visual description (cached): periodic screenshot → VisionDescriptionService
 *
 * The visual tier lets text-only models "see" the live room — camera feeds,
 * avatars, screen shares, the grid layout. Vision descriptions are cached by
 * LiveRoomSnapshotService (30s interval, content-addressed dedup).
 *
 * Priority 30, budget 3%. Active only when there's a live call.
 *
 * Query strategy: find ANY active call (global cache, one query per 10s).
 * 14+ personas share one cached result instead of firing 28 parallel queries.
 */

import type { RAGSource, RAGSection, RAGSourceContext } from '../shared/RAGSource';
import type { RAGArtifact } from '../shared/RAGTypes';
import { ORM } from '../../../daemons/data-daemon/server/ORM';
import { CallEntity, type CallParticipant } from '../../data/entities/CallEntity';
import { LiveRoomSnapshotService, type RoomSnapshot } from '../../live/LiveRoomSnapshotService';
import { Logger } from '../../core/logging/Logger';
import * as fs from 'fs';

const log = Logger.create('LiveRoomAwarenessSource', 'rag');

export class LiveRoomAwarenessSource implements RAGSource {
  readonly name = 'live-room-awareness';
  readonly priority = 30;
  readonly defaultBudgetPercent = 3;

  /** Global active call cache — ONE query shared by all 14+ personas */
  private static _globalCallCache: { call: CallEntity | null; expiresAt: number } | null = null;
  private static readonly CACHE_TTL_MS = 10_000;

  /** Fire-and-forget query that populates cache asynchronously */
  private static _queryRunning = false;

  isApplicable(_context: RAGSourceContext): boolean {
    return true;
  }

  async load(context: RAGSourceContext, allocatedBudget: number): Promise<RAGSection> {
    const startTime = performance.now();

    const call = this.getActiveCall();
    if (!call) {
      return this.emptySection(performance.now() - startTime);
    }

    const participants = call.getActiveParticipants();
    if (participants.length === 0) {
      return this.emptySection(performance.now() - startTime);
    }

    // Tier 1: participant metadata (always available, free)
    const participantSection = this.buildParticipantSection(participants, context.personaId);

    // Tier 2: visual description (from snapshot service cache, non-blocking)
    const snapshotService = LiveRoomSnapshotService.getInstance();
    const snapshot = snapshotService.getCachedDescription(call.roomId);

    // Always include both: text description for text-only models,
    // raw screenshot artifact for vision models. PersonaResponseGenerator
    // decides which to use based on actual model capabilities.
    const artifacts = await this.buildArtifacts(snapshot);

    const visualSection = snapshot
      ? `\nVisual: ${snapshot.description}`
      : '';

    const fullSection = participantSection + visualSection + '\n=================';
    const tokenCount = Math.ceil(fullSection.length / 4);

    if (tokenCount > allocatedBudget && allocatedBudget < 30) {
      return this.emptySection(performance.now() - startTime);
    }

    const loadTimeMs = performance.now() - startTime;
    log.info(`${participants.length} participants, visual=${!!snapshot} (${loadTimeMs.toFixed(1)}ms)`);

    return {
      sourceName: this.name,
      tokenCount: Math.min(tokenCount, allocatedBudget),
      loadTimeMs,
      systemPromptSection: tokenCount > allocatedBudget
        ? fullSection.slice(0, allocatedBudget * 4 - 40) + '\n[truncated]'
        : fullSection,
      artifacts: artifacts.length > 0 ? artifacts : undefined,
    };
  }

  private buildParticipantSection(participants: CallParticipant[], selfId: string): string {
    const lines: string[] = ['\n=== LIVE ROOM ==='];
    lines.push(`You are in a live call with ${participants.length} participant(s):`);

    for (const p of participants) {
      const isSelf = p.userId === selfId;
      const media: string[] = [];
      if (p.micEnabled) media.push('mic on');
      if (p.cameraEnabled) media.push('camera on');
      if (p.screenShareEnabled) media.push('screen sharing');
      if (media.length === 0) media.push('observing');

      const name = isSelf ? `${p.displayName} (you)` : p.displayName;
      lines.push(`- ${name}: ${media.join(', ')}`);
    }

    return lines.join('\n');
  }

  private async buildArtifacts(snapshot: RoomSnapshot | null): Promise<RAGArtifact[]> {
    if (!snapshot?.snapshotPath) return [];

    // Read base64 from disk lazily — NOT held in RAM between cycles
    let base64: string;
    try {
      const buffer = await fs.promises.readFile(snapshot.snapshotPath);
      base64 = buffer.toString('base64');
    } catch {
      return []; // File gone or unreadable
    }

    return [{
      type: 'screenshot',
      base64,
      content: snapshot.description,
      metadata: {
        source: 'live-room-snapshot',
        capturedAt: snapshot.capturedAt,
        mimeType: snapshot.mimeType,
      },
      preprocessed: {
        type: 'image_description',
        result: snapshot.description,
        confidence: 0.9,
        processingTime: snapshot.visionTimeMs,
        model: `${snapshot.provider}/${snapshot.modelId}`,
      },
    }];
  }

  /**
   * Non-blocking: return cached call or null.
   * Kicks off a background query to populate the cache if expired.
   * Never blocks the RAG pipeline — first call returns null, subsequent calls
   * return the cached result once the background query completes.
   *
   * This eliminates the timeout problem: 14 personas all get instant null/cached
   * results, and exactly ONE background query runs to refresh the cache.
   */
  private getActiveCall(): CallEntity | null {
    const cached = LiveRoomAwarenessSource._globalCallCache;
    if (cached && performance.now() < cached.expiresAt) {
      return cached.call;
    }

    // Fire background query (fire-and-forget, no await)
    if (!LiveRoomAwarenessSource._queryRunning) {
      LiveRoomAwarenessSource._queryRunning = true;
      this.refreshActiveCallCache().finally(() => {
        LiveRoomAwarenessSource._queryRunning = false;
      });
    }

    // Return stale cache if available (better than null during refresh)
    return cached?.call ?? null;
  }

  private async refreshActiveCallCache(): Promise<void> {
    try {
      const result = await ORM.query<CallEntity>({
        collection: CallEntity.collection,
        filter: { status: 'active' },
        limit: 1,
      }, 'default');

      if (!result.success || !result.data?.length) {
        LiveRoomAwarenessSource._globalCallCache = {
          call: null,
          expiresAt: performance.now() + LiveRoomAwarenessSource.CACHE_TTL_MS,
        };
        return;
      }

      const entity = new CallEntity();
      Object.assign(entity, result.data[0].data);
      LiveRoomAwarenessSource._globalCallCache = {
        call: entity,
        expiresAt: performance.now() + LiveRoomAwarenessSource.CACHE_TTL_MS,
      };
      log.info(`Found active call ${entity.id?.slice(0, 8)} with ${entity.getActiveParticipants().length} participants`);
    } catch (error) {
      log.warn('Failed to query active call:', error);
      LiveRoomAwarenessSource._globalCallCache = {
        call: null,
        expiresAt: performance.now() + LiveRoomAwarenessSource.CACHE_TTL_MS,
      };
    }
  }

  private emptySection(loadTimeMs: number): RAGSection {
    return {
      sourceName: this.name,
      tokenCount: 0,
      loadTimeMs,
    };
  }
}
