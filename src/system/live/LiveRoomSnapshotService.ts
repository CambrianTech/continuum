/**
 * LiveRoomSnapshotService — Visual awareness of the live room
 *
 * Provides AI personas with a visual description of the live room by
 * capturing video frames directly from LiveKit via Rust IPC:
 *
 * 1. Rust VideoFrameCapture subscribes to LiveKit video tracks (humans + avatars)
 * 2. Captures latest frame per participant as JPEG (1 fps, content-addressed dedup)
 * 3. Composes a grid snapshot of all participants on demand
 * 4. VisionDescriptionService generates text description for text-only models
 *
 * Architecture:
 * - Lazy: first getDescription() call starts capture
 * - Content-addressed: skips re-description if video frames unchanged
 * - Single in-flight: prevents pileup when multiple personas request simultaneously
 * - Auto-stop: stops when no readers for 5 minutes
 * - Interval: 30s default
 *
 * The Rust capture replaces browser screenshots — it captures directly from
 * LiveKit video tracks, showing actual camera feeds instead of dark tiles.
 */

import { Commands } from '../core/shared/Commands';
import { VisionDescriptionService } from '../vision/VisionDescriptionService';
import { Logger } from '../core/logging/Logger';
import { createHash } from 'crypto';

const log = Logger.create('LiveRoomSnapshotService', 'rag');

export interface RoomSnapshot {
  description: string;
  base64: string;
  mimeType: string;
  capturedAt: number;
  hash: string;
  provider: string;
  modelId: string;
  visionTimeMs: number;
}

const CAPTURE_INTERVAL_MS = 30_000;
const CACHE_TTL_MS = 90_000;
const IDLE_STOP_MS = 5 * 60_000;
const VISION_TIMEOUT_MS = 20_000;

export class LiveRoomSnapshotService {
  private static _instance: LiveRoomSnapshotService | null = null;

  private _cache = new Map<string, RoomSnapshot>();
  private _captureInFlight = new Set<string>();
  private _intervalHandles = new Map<string, ReturnType<typeof setInterval>>();
  private _lastReadAt = new Map<string, number>();
  private _lastHash = new Map<string, string>();

  static getInstance(): LiveRoomSnapshotService {
    if (!this._instance) {
      this._instance = new LiveRoomSnapshotService();
    }
    return this._instance;
  }

  /**
   * Non-blocking: return cached description or null.
   * Starts the capture loop in background if not running.
   * RAGSources call this — they should never block on vision.
   */
  getCachedDescription(roomId: string): RoomSnapshot | null {
    this._lastReadAt.set(roomId, Date.now());

    // Start capture loop if not running (fire-and-forget)
    if (!this._intervalHandles.has(roomId)) {
      this.startCaptureLoop(roomId);
    }

    const cached = this._cache.get(roomId);
    if (cached && Date.now() - cached.capturedAt < CACHE_TTL_MS) {
      return cached;
    }

    return null;
  }

  /**
   * Blocking: wait for a description (used by tests or explicit requests).
   */
  async getDescription(roomId: string): Promise<RoomSnapshot | null> {
    this._lastReadAt.set(roomId, Date.now());

    if (!this._intervalHandles.has(roomId)) {
      this.startCaptureLoop(roomId);
    }

    const cached = this._cache.get(roomId);
    if (cached && Date.now() - cached.capturedAt < CACHE_TTL_MS) {
      return cached;
    }

    if (!this._captureInFlight.has(roomId)) {
      await this.captureAndDescribe(roomId);
    }

    return this._cache.get(roomId) ?? null;
  }

  stopAll(): void {
    for (const [roomId, handle] of this._intervalHandles) {
      clearInterval(handle);
      log.info(`Stopped capture loop for room ${roomId}`);
    }
    this._intervalHandles.clear();
    this._cache.clear();
    this._captureInFlight.clear();
    this._lastReadAt.clear();
    this._lastHash.clear();
  }

  private startCaptureLoop(roomId: string): void {
    log.info(`Starting visual capture loop for room ${roomId} (interval=${CAPTURE_INTERVAL_MS / 1000}s)`);

    const tick = async () => {
      const lastRead = this._lastReadAt.get(roomId) ?? 0;
      if (Date.now() - lastRead > IDLE_STOP_MS) {
        log.info(`No readers for ${IDLE_STOP_MS / 1000}s, stopping capture for room ${roomId}`);
        const handle = this._intervalHandles.get(roomId);
        if (handle) clearInterval(handle);
        this._intervalHandles.delete(roomId);
        this._cache.delete(roomId);
        return;
      }

      await this.captureAndDescribe(roomId);
    };

    // Immediate first capture
    tick();

    const handle = setInterval(tick, CAPTURE_INTERVAL_MS);
    this._intervalHandles.set(roomId, handle);
  }

  private async captureAndDescribe(roomId: string): Promise<void> {
    if (this._captureInFlight.has(roomId)) return;
    this._captureInFlight.add(roomId);

    try {
      // Capture from Rust VideoFrameCapture via IPC.
      // This reads directly from LiveKit video tracks — no browser screenshots.
      const base64 = await this.captureFromRust();
      if (!base64) {
        log.debug('No video frames captured from LiveKit participants');
        return;
      }
      log.info(`Captured room snapshot: ${Math.ceil(base64.length / 1024)}KB base64`);

      // Content hash — skip vision if scene unchanged
      const hash = createHash('sha256').update(base64.slice(0, 8192)).digest('hex').slice(0, 16);
      if (hash === this._lastHash.get(roomId)) {
        const existing = this._cache.get(roomId);
        if (existing) {
          this._cache.set(roomId, { ...existing, capturedAt: Date.now() });
        }
        return;
      }

      // Describe via vision model
      const visionService = VisionDescriptionService.getInstance();
      if (!visionService.isAvailable()) {
        log.warn('No vision model available for room description');
        return;
      }
      log.info(`Vision available, sending ${Math.ceil(base64.length / 1024)}KB to VisionDescriptionService...`);

      const visionStart = performance.now();
      const result = await Promise.race([
        visionService.describeBase64(base64, 'image/jpeg', {
          maxLength: 400,
          detectText: true,
          prompt: [
            'Describe this live video room snapshot concisely.',
            'Note: who has their camera on (real video vs avatar),',
            'the room layout, number of participants visible,',
            'any screen shares or visual content.',
            'Focus on what would be useful for a participant who cannot see the screen.',
          ].join(' '),
        }),
        new Promise<null>((resolve) => setTimeout(() => resolve(null), VISION_TIMEOUT_MS)),
      ]);

      if (!result) {
        log.warn('Vision description timed out');
        return;
      }

      const visionTimeMs = performance.now() - visionStart;

      this._cache.set(roomId, {
        description: result.description,
        base64,
        mimeType: 'image/jpeg',
        capturedAt: Date.now(),
        hash,
        provider: result.provider,
        modelId: result.modelId,
        visionTimeMs,
      });
      this._lastHash.set(roomId, hash);

      log.info(`Room visual: ${visionTimeMs.toFixed(0)}ms via ${result.modelId} — "${result.description.slice(0, 100)}..."`);
    } catch (error) {
      log.error('Capture/describe failed:', error);
    } finally {
      this._captureInFlight.delete(roomId);
    }
  }

  /**
   * Capture room snapshot from Rust VideoFrameCapture via IPC.
   * The Rust service subscribes to LiveKit video tracks directly —
   * no browser needed, actual camera feeds captured.
   *
   * Returns base64 JPEG or null if no participants have video.
   */
  private async captureFromRust(): Promise<string | null> {
    try {
      const result = await Commands.execute('voice/snapshot-room', {}) as unknown as {
        success: boolean;
        base64?: string;
        error?: string;
      };

      if (!result.success || !result.base64) {
        log.debug(`Rust snapshot: ${result.error ?? 'no base64 data'}`);
        return null;
      }

      return result.base64;
    } catch (error) {
      log.debug('Rust snapshot IPC error:', error);
      return null;
    }
  }
}
