/**
 * LiveRoomSnapshotService — Visual awareness of the live room
 *
 * Provides AI personas with a visual description of the live room by
 * composing data from multiple sources:
 *
 * 1. Participant metadata from CallEntity (free, always available)
 * 2. Avatar snapshots from Bevy via `avatar/snapshot` Rust IPC (cached PNGs)
 * 3. Composite room screenshot via browser `interface/screenshot` (human camera + layout)
 *
 * The composite screenshot is the key visual — it shows the actual room as
 * rendered in the browser: human camera feeds, avatar tiles, grid layout.
 * This gets described via VisionDescriptionService and cached.
 *
 * Architecture:
 * - Lazy: first getDescription() call starts capture
 * - Content-addressed: skips re-description if screenshot bytes unchanged
 * - Single in-flight: prevents pileup when multiple personas request simultaneously
 * - Auto-stop: stops when no readers for 5 minutes
 * - Interval: 30s default, 60s when scene is static
 */

import { Commands } from '../core/shared/Commands';
import { VisionDescriptionService } from '../vision/VisionDescriptionService';
import { Logger } from '../core/logging/Logger';
import type { ScreenshotParams, ScreenshotResult } from '../../commands/interface/screenshot/shared/ScreenshotTypes';
import { createHash } from 'crypto';
import * as fs from 'fs';

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
      // Capture the live-widget composite from browser.
      // This shows the actual rendered room: human camera, avatar tiles, grid layout.
      // The browser already has all the LiveKit video tracks attached to DOM elements.
      const base64 = await this.captureLiveWidget();
      if (!base64) {
        log.warn('No screenshot data received from live-widget capture');
        return;
      }
      log.info(`Captured live-widget: ${Math.ceil(base64.length / 1024)}KB base64`);

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
            'Describe this live video room screenshot concisely.',
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
   * Capture the live-widget from the browser. This is the composite view showing
   * all LiveKit video tracks (human cameras + avatar renders) in the grid layout.
   * The browser already subscribes to all tracks via AudioStreamClient — we just
   * screenshot what's rendered.
   *
   * Returns base64 JPEG or null on failure.
   */
  private async captureLiveWidget(): Promise<string | null> {
    try {
      // Full page capture — live-widget is inside Shadow DOM so querySelector fails.
      // Full page includes sidebar + grid + controls, but vision model can parse it.
      const result = await Commands.execute<ScreenshotParams, ScreenshotResult>('interface/screenshot', {
        resultType: 'both',
        format: 'jpeg',
        quality: 50,
        scale: 0.35,
        filename: `live-snapshot-${Date.now()}.jpg`,
      } as Partial<ScreenshotParams>);

      if (!result.success) {
        log.debug(`Screenshot capture failed: ${result.error?.message ?? 'unknown'}`);
        return null;
      }

      // dataUrl may be populated (browser returns it) or null (server strips for non-persona callers).
      // Fall back to reading the saved file from disk.
      if (result.dataUrl) {
        return result.dataUrl.replace(/^data:image\/\w+;base64,/, '');
      }

      if (result.filepath) {
        try {
          const fileData = fs.readFileSync(result.filepath);
          return fileData.toString('base64');
        } catch {
          log.debug(`Could not read saved screenshot from ${result.filepath}`);
          return null;
        }
      }

      log.debug('Screenshot succeeded but no dataUrl or filepath');
      return null;
    } catch (error) {
      log.debug('Screenshot command error:', error);
      return null;
    }
  }
}
