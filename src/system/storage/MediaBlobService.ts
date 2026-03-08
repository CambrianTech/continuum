/**
 * MediaBlobService - Externalizes binary media from entities to content-addressed blob storage
 *
 * Problem: Base64-encoded images (100KB-1MB+) stored inline as JSON text in SQLite
 * bloat the database and slow down every query that touches the entity.
 *
 * Solution: Store binary media as files on disk, reference by content hash.
 * The HTTP server serves files at /media/{hash}.{ext}, so the browser can
 * render them via <img src="/media/sha256:abc123.png"> — no base64 in the entity.
 *
 * Flow:
 *   Store: base64 → decode → write file → set url + blobHash, clear base64
 *   Serve: /media/{hash}.{ext} → read file → respond with binary + Content-Type
 */

import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import { SystemPaths } from '../core/config/SystemPaths';
import type { MediaItem } from '../data/entities/ChatMessageEntity';

/** Minimum base64 size worth externalizing (4KB decoded = ~5.3KB base64) */
const MIN_EXTERNALIZE_SIZE = 5000;

/**
 * MediaBlobService - Content-addressed storage for binary media
 */
export class MediaBlobService {
  private static _mediaDir: string | null = null;

  /** Get the media storage directory, creating it if needed */
  private static get mediaDir(): string {
    if (!this._mediaDir) {
      this._mediaDir = path.join(SystemPaths.blobs.root, 'media');
      if (!fs.existsSync(this._mediaDir)) {
        fs.mkdirSync(this._mediaDir, { recursive: true });
      }
    }
    return this._mediaDir;
  }

  /**
   * Externalize media items: move base64 data to disk, set URL for HTTP serving.
   *
   * Mutates the MediaItem array in place:
   * - Sets `blobHash` to content hash
   * - Sets `url` to `/media/{hash}.{ext}` for browser rendering
   * - Clears `base64` to free the inline data
   *
   * @returns Array of hashes that were stored (for tracking/cleanup)
   */
  static async externalize(mediaItems: MediaItem[]): Promise<string[]> {
    const stored: string[] = [];

    for (const item of mediaItems) {
      if (!item.base64 || item.base64.length < MIN_EXTERNALIZE_SIZE) {
        continue; // Too small to bother externalizing
      }

      const hash = this.computeHash(item.base64);
      const ext = this.extensionFromMime(item.mimeType);
      const filePath = this.getFilePath(hash);

      // Deduplicate — skip write if file already exists
      if (!fs.existsSync(filePath)) {
        const buffer = Buffer.from(item.base64, 'base64');
        const dir = path.dirname(filePath);
        if (!fs.existsSync(dir)) {
          fs.mkdirSync(dir, { recursive: true });
        }
        // Atomic write: temp file → rename
        const tempPath = `${filePath}.tmp.${Date.now()}`;
        await fs.promises.writeFile(tempPath, buffer);
        await fs.promises.rename(tempPath, filePath);
      }

      // Update the MediaItem: set URL for HTTP serving, clear inline data
      item.blobHash = hash;
      item.url = `/media/${hash}${ext}`;
      item.size = item.size ?? Buffer.from(item.base64, 'base64').length;
      item.base64 = undefined;
      stored.push(hash);
    }

    return stored;
  }

  /**
   * Retrieve a media blob file path by hash.
   * Returns null if the blob doesn't exist.
   */
  static getPath(hash: string): string | null {
    const filePath = this.getFilePath(hash);
    return fs.existsSync(filePath) ? filePath : null;
  }

  /**
   * Retrieve raw binary data by hash.
   */
  static async retrieve(hash: string): Promise<Buffer | null> {
    const filePath = this.getFilePath(hash);
    if (!fs.existsSync(filePath)) return null;
    return fs.promises.readFile(filePath);
  }

  /**
   * Check if a media blob exists.
   */
  static exists(hash: string): boolean {
    return fs.existsSync(this.getFilePath(hash));
  }

  // ── Internal ────────────────────────────────────────────────────────

  private static computeHash(base64: string): string {
    // Hash the raw binary content (not the base64 encoding)
    const buffer = Buffer.from(base64, 'base64');
    const hex = crypto.createHash('sha256').update(buffer).digest('hex');
    return `sha256:${hex}`;
  }

  private static getFilePath(hash: string): string {
    const hex = hash.replace('sha256:', '');
    // Shard by first 2 chars (256 directories, matches BlobStorage pattern)
    const shard = hex.substring(0, 2);
    const filename = hex.substring(2);
    return path.join(this.mediaDir, shard, filename);
  }

  private static extensionFromMime(mimeType?: string): string {
    if (!mimeType) return '.bin';
    const map: Record<string, string> = {
      'image/png': '.png',
      'image/jpeg': '.jpg',
      'image/gif': '.gif',
      'image/webp': '.webp',
      'image/svg+xml': '.svg',
      'audio/mpeg': '.mp3',
      'audio/wav': '.wav',
      'audio/ogg': '.ogg',
      'video/mp4': '.mp4',
      'video/webm': '.webm',
      'application/pdf': '.pdf',
    };
    return map[mimeType] ?? '.bin';
  }
}
