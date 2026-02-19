/**
 * BlobStorage - Content-addressable storage for large JSON blobs
 *
 * Problem: SQLite is not designed for large JSON blobs (coordination_decisions
 * has 262MB of rag_context data averaging 73KB per row).
 *
 * Solution: Store large blobs externally, reference by content hash:
 * - Content-addressable: Same content = same hash = automatic deduplication
 * - Compressed: gzip reduces storage by 60-80%
 * - External: File system (local) or S3-compatible (future)
 * - Lazy loading: Only fetch when needed
 *
 * Usage:
 * ```typescript
 * // Store a large object
 * const ref = await BlobStorage.store(largeRAGContext);
 * // ref = { hash: "sha256:abc123...", size: 73000, compressedSize: 18000 }
 *
 * // Store reference in database instead of full object
 * entity.ragContextRef = ref.hash;
 *
 * // Retrieve later
 * const ragContext = await BlobStorage.retrieve(entity.ragContextRef);
 * ```
 */

import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import * as zlib from 'zlib';
import { promisify } from 'util';

const gzip = promisify(zlib.gzip);
const gunzip = promisify(zlib.gunzip);

/**
 * Reference to a stored blob
 */
export interface BlobRef {
  /** Content hash (sha256:hex) */
  hash: string;
  /** Original size in bytes */
  size: number;
  /** Compressed size in bytes */
  compressedSize: number;
  /** Storage backend used */
  backend: 'file' | 's3';
  /** When stored */
  storedAt: string;
}

/**
 * Blob storage statistics
 */
export interface BlobStorageStats {
  totalBlobs: number;
  totalSizeBytes: number;
  totalCompressedBytes: number;
  compressionRatio: number;
  oldestBlob?: string;
  newestBlob?: string;
}

/**
 * BlobStorage configuration
 */
export interface BlobStorageConfig {
  /** Base directory for file storage */
  basePath: string;
  /** Minimum size to store as blob (smaller objects stay inline) */
  minBlobSize: number;
  /** Enable compression */
  compress: boolean;
}

const DEFAULT_CONFIG: BlobStorageConfig = {
  basePath: '.continuum/blobs',
  minBlobSize: 4096,  // 4KB - smaller stays inline
  compress: true,
};

/**
 * BlobStorage - Content-addressable storage for large data
 */
export class BlobStorage {
  private static config: BlobStorageConfig = DEFAULT_CONFIG;
  private static initialized = false;

  /**
   * Initialize blob storage with custom config
   */
  static initialize(config: Partial<BlobStorageConfig> = {}): void {
    this.config = { ...DEFAULT_CONFIG, ...config };
    this.ensureDirectory();
    this.initialized = true;
  }

  /**
   * Store a JSON object as a blob
   *
   * @param data - Object to store
   * @returns Blob reference (store this in your database)
   */
  static async store<T>(data: T): Promise<BlobRef> {
    this.ensureInitialized();

    // Serialize to JSON
    const json = JSON.stringify(data);
    const originalSize = Buffer.byteLength(json, 'utf8');

    // Skip blob storage for small objects
    if (originalSize < this.config.minBlobSize) {
      throw new Error(`Object too small for blob storage (${originalSize} < ${this.config.minBlobSize}). Store inline instead.`);
    }

    // Calculate content hash
    const hash = this.computeHash(json);

    // Check if already exists (deduplication)
    const filePath = this.getFilePath(hash);
    if (fs.existsSync(filePath)) {
      const stats = fs.statSync(filePath);
      return {
        hash,
        size: originalSize,
        compressedSize: stats.size,
        backend: 'file',
        storedAt: stats.mtime.toISOString(),
      };
    }

    // Compress and store
    let content: Buffer;
    let compressedSize: number;

    if (this.config.compress) {
      content = await gzip(Buffer.from(json, 'utf8'));
      compressedSize = content.length;
    } else {
      content = Buffer.from(json, 'utf8');
      compressedSize = originalSize;
    }

    // Write atomically (write to temp, then rename)
    const tempPath = `${filePath}.tmp.${Date.now()}`;
    await fs.promises.writeFile(tempPath, content);
    await fs.promises.rename(tempPath, filePath);

    return {
      hash,
      size: originalSize,
      compressedSize,
      backend: 'file',
      storedAt: new Date().toISOString(),
    };
  }

  /**
   * Retrieve a blob by hash
   *
   * @param hash - Blob hash (from BlobRef)
   * @returns Original object, or null if not found
   */
  static async retrieve<T>(hash: string): Promise<T | null> {
    this.ensureInitialized();

    const filePath = this.getFilePath(hash);
    if (!fs.existsSync(filePath)) {
      return null;
    }

    const content = await fs.promises.readFile(filePath);

    let json: string;
    if (this.config.compress) {
      const decompressed = await gunzip(content);
      json = decompressed.toString('utf8');
    } else {
      json = content.toString('utf8');
    }

    return JSON.parse(json) as T;
  }

  /**
   * Check if a blob exists
   */
  static exists(hash: string): boolean {
    this.ensureInitialized();
    return fs.existsSync(this.getFilePath(hash));
  }

  /**
   * Delete a blob
   */
  static async delete(hash: string): Promise<boolean> {
    this.ensureInitialized();

    const filePath = this.getFilePath(hash);
    if (!fs.existsSync(filePath)) {
      return false;
    }

    await fs.promises.unlink(filePath);
    return true;
  }

  /**
   * Get storage statistics
   */
  static async getStats(): Promise<BlobStorageStats> {
    this.ensureInitialized();

    const blobDir = this.config.basePath;
    if (!fs.existsSync(blobDir)) {
      return {
        totalBlobs: 0,
        totalSizeBytes: 0,
        totalCompressedBytes: 0,
        compressionRatio: 1,
      };
    }

    let totalBlobs = 0;
    let totalCompressedBytes = 0;
    let oldestTime = Infinity;
    let newestTime = 0;
    let oldestBlob: string | undefined;
    let newestBlob: string | undefined;

    // Walk through shard directories
    const shards = await fs.promises.readdir(blobDir);
    for (const shard of shards) {
      const shardPath = path.join(blobDir, shard);
      const stat = await fs.promises.stat(shardPath);
      if (!stat.isDirectory()) continue;

      const files = await fs.promises.readdir(shardPath);
      for (const file of files) {
        if (!file.endsWith('.blob')) continue;

        const filePath = path.join(shardPath, file);
        const fileStat = await fs.promises.stat(filePath);

        totalBlobs++;
        totalCompressedBytes += fileStat.size;

        if (fileStat.mtimeMs < oldestTime) {
          oldestTime = fileStat.mtimeMs;
          oldestBlob = file.replace('.blob', '');
        }
        if (fileStat.mtimeMs > newestTime) {
          newestTime = fileStat.mtimeMs;
          newestBlob = file.replace('.blob', '');
        }
      }
    }

    // Estimate original size based on typical compression ratio (~4:1 for JSON)
    const estimatedOriginalSize = totalCompressedBytes * 4;

    return {
      totalBlobs,
      totalSizeBytes: estimatedOriginalSize,
      totalCompressedBytes,
      compressionRatio: totalCompressedBytes > 0 ? estimatedOriginalSize / totalCompressedBytes : 1,
      oldestBlob,
      newestBlob,
    };
  }

  /**
   * Prune old blobs not referenced by any database record
   * (Call this periodically or after cleanup operations)
   *
   * @param referencedHashes - Set of hashes still in use
   * @returns Number of blobs deleted
   */
  static async prune(referencedHashes: Set<string>): Promise<number> {
    this.ensureInitialized();

    let deleted = 0;
    const blobDir = this.config.basePath;

    if (!fs.existsSync(blobDir)) {
      return 0;
    }

    const shards = await fs.promises.readdir(blobDir);
    for (const shard of shards) {
      const shardPath = path.join(blobDir, shard);
      const stat = await fs.promises.stat(shardPath);
      if (!stat.isDirectory()) continue;

      const files = await fs.promises.readdir(shardPath);
      for (const file of files) {
        if (!file.endsWith('.blob')) continue;

        const hash = `sha256:${shard}${file.replace('.blob', '')}`;
        if (!referencedHashes.has(hash)) {
          await fs.promises.unlink(path.join(shardPath, file));
          deleted++;
        }
      }
    }

    return deleted;
  }

  // --- Private methods ---

  private static computeHash(content: string): string {
    const hash = crypto.createHash('sha256').update(content).digest('hex');
    return `sha256:${hash}`;
  }

  private static getFilePath(hash: string): string {
    // Extract hex hash from "sha256:abc123..."
    const hexHash = hash.replace('sha256:', '');

    // Shard by first 2 chars (256 directories)
    const shard = hexHash.substring(0, 2);
    const filename = hexHash.substring(2) + '.blob';

    return path.join(this.config.basePath, shard, filename);
  }

  private static ensureDirectory(): void {
    if (!fs.existsSync(this.config.basePath)) {
      fs.mkdirSync(this.config.basePath, { recursive: true });
    }
  }

  private static ensureInitialized(): void {
    if (!this.initialized) {
      this.initialize();
    }
  }
}

/**
 * Helper to conditionally store large objects as blobs
 *
 * Usage:
 * ```typescript
 * // Returns inline data for small objects, BlobRef for large
 * const result = await storeIfLarge(ragContext);
 * if (result.isBlob) {
 *   entity.ragContextRef = result.ref.hash;
 *   entity.ragContext = null;
 * } else {
 *   entity.ragContext = result.data;
 *   entity.ragContextRef = null;
 * }
 * ```
 */
export async function storeIfLarge<T>(
  data: T,
  threshold: number = 4096
): Promise<{ isBlob: true; ref: BlobRef } | { isBlob: false; data: T }> {
  const json = JSON.stringify(data);
  const size = Buffer.byteLength(json, 'utf8');

  if (size >= threshold) {
    const ref = await BlobStorage.store(data);
    return { isBlob: true, ref };
  }

  return { isBlob: false, data };
}

/**
 * Helper to retrieve data that might be inline or in blob storage
 */
export async function retrieveOrInline<T>(
  inlineData: T | null | undefined,
  blobRef: string | null | undefined
): Promise<T | null> {
  if (inlineData) {
    return inlineData;
  }

  if (blobRef) {
    return await BlobStorage.retrieve<T>(blobRef);
  }

  return null;
}
