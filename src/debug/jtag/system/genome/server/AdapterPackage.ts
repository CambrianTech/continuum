/**
 * AdapterPackage - Standardized packaging for trained LoRA adapters
 *
 * Handles:
 * - Writing/reading manifest.json files in adapter directories
 * - Calculating directory size and content hashes
 * - Converting manifests to GenomeLayerEntity instances
 * - Scanning adapter directories for existing packages
 *
 * Each adapter directory follows a standard layout:
 *   .continuum/genome/adapters/{name}-{timestamp}/
 *   ├── manifest.json                    ← Package metadata
 *   ├── adapter_model.safetensors        ← PEFT weights
 *   ├── adapter_config.json              ← LoRA config
 *   └── ...                              ← Other PEFT output files
 *
 * SERVER-ONLY: Uses Node.js fs, path, crypto APIs
 */

import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { generateUUID } from '../../core/types/CrossPlatformUUID';
import { GenomeLayerEntity } from '../entities/GenomeLayerEntity';
import type { TrainingMetadata } from '../entities/GenomeLayerEntity';
import type { AdapterPackageManifest } from '../shared/AdapterPackageTypes';

// Re-export for convenience
export type { AdapterPackageManifest } from '../shared/AdapterPackageTypes';

const MANIFEST_FILENAME = 'manifest.json';

/**
 * AdapterPackage — Static utility class for adapter packaging operations
 */
export class AdapterPackage {

  /**
   * Write manifest.json to an adapter directory
   */
  static async writeManifest(adapterPath: string, manifest: AdapterPackageManifest): Promise<void> {
    const manifestPath = path.join(adapterPath, MANIFEST_FILENAME);
    await fs.promises.writeFile(manifestPath, JSON.stringify(manifest, null, 2), 'utf-8');
  }

  /**
   * Read manifest.json from an adapter directory
   *
   * @throws Error if manifest doesn't exist or is malformed
   */
  static async readManifest(adapterPath: string): Promise<AdapterPackageManifest> {
    const manifestPath = path.join(adapterPath, MANIFEST_FILENAME);
    const content = await fs.promises.readFile(manifestPath, 'utf-8');
    return JSON.parse(content) as AdapterPackageManifest;
  }

  /**
   * Calculate the total size of an adapter directory in megabytes
   */
  static async calculateSizeMB(adapterPath: string): Promise<number> {
    const totalBytes = await this.calculateDirSize(adapterPath);
    return Math.round((totalBytes / (1024 * 1024)) * 100) / 100;
  }

  /**
   * Calculate SHA-256 hash of the primary weights file (adapter_model.safetensors)
   * Falls back to hashing adapter_config.json if weights file doesn't exist.
   *
   * @returns hex-encoded SHA-256 hash prefixed with "sha256:"
   */
  static async calculateContentHash(adapterPath: string): Promise<string> {
    // Try primary weights file first
    const weightsPath = path.join(adapterPath, 'adapter_model.safetensors');
    if (fs.existsSync(weightsPath)) {
      return this.hashFile(weightsPath);
    }

    // Fallback: hash adapter_config.json
    const configPath = path.join(adapterPath, 'adapter_config.json');
    if (fs.existsSync(configPath)) {
      return this.hashFile(configPath);
    }

    // Last resort: hash all file names + sizes as a fingerprint
    return this.hashDirectoryFingerprint(adapterPath);
  }

  /**
   * Create a GenomeLayerEntity from a manifest and adapter path
   */
  static toGenomeLayerEntity(manifest: AdapterPackageManifest, adapterPath: string): GenomeLayerEntity {
    const entity = new GenomeLayerEntity();

    entity.id = manifest.id;
    entity.name = manifest.name;
    entity.description = `LoRA adapter for ${manifest.personaName} (${manifest.traitType}), base model: ${manifest.baseModel}`;
    entity.traitType = manifest.traitType;
    entity.source = manifest.source;
    entity.modelPath = adapterPath;
    entity.sizeMB = manifest.sizeMB;
    entity.rank = manifest.rank;
    entity.creatorId = manifest.personaId;
    entity.trainingMetadata = manifest.trainingMetadata;
    entity.contentHash = manifest.contentHash;
    entity.tags = [manifest.traitType, manifest.baseModel, manifest.personaName.toLowerCase()];
    entity.generation = 0;

    const createdAt = new Date(manifest.createdAt);
    entity.createdAt = createdAt;
    entity.updatedAt = createdAt;

    return entity;
  }

  /**
   * Build a manifest from training results
   */
  static buildManifest(params: {
    adapterPath: string;
    personaId: UUID;
    personaName: string;
    traitType: string;
    baseModel: string;
    rank: number;
    sizeMB: number;
    contentHash?: string;
    trainingMetadata: TrainingMetadata;
  }): AdapterPackageManifest {
    const safeName = params.personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-');

    return {
      id: generateUUID(),
      name: `${safeName}-${params.traitType}`,
      traitType: params.traitType,
      source: 'trained',
      baseModel: params.baseModel,
      rank: params.rank,
      sizeMB: params.sizeMB,
      personaId: params.personaId,
      personaName: params.personaName,
      trainingMetadata: params.trainingMetadata,
      contentHash: params.contentHash,
      createdAt: new Date().toISOString(),
      version: 1,
    };
  }

  /**
   * Scan a base directory for adapter packages (directories containing manifest.json)
   */
  static async scanAdapterDirectory(baseDir: string): Promise<AdapterPackageManifest[]> {
    if (!fs.existsSync(baseDir)) {
      return [];
    }

    const manifests: AdapterPackageManifest[] = [];
    const entries = await fs.promises.readdir(baseDir, { withFileTypes: true });

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;

      const adapterPath = path.join(baseDir, entry.name);
      const manifestPath = path.join(adapterPath, MANIFEST_FILENAME);

      if (fs.existsSync(manifestPath)) {
        try {
          const manifest = await this.readManifest(adapterPath);
          manifests.push(manifest);
        } catch (error) {
          console.warn(`Failed to read manifest at ${manifestPath}:`, error);
        }
      }
    }

    return manifests;
  }

  // ==================== Private Helpers ====================

  /**
   * Recursively calculate total byte size of a directory
   */
  private static async calculateDirSize(dirPath: string): Promise<number> {
    let totalSize = 0;
    const entries = await fs.promises.readdir(dirPath, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = path.join(dirPath, entry.name);
      if (entry.isDirectory()) {
        totalSize += await this.calculateDirSize(fullPath);
      } else {
        const stats = await fs.promises.stat(fullPath);
        totalSize += stats.size;
      }
    }

    return totalSize;
  }

  /**
   * SHA-256 hash a file, returning "sha256:{hex}" format
   */
  private static async hashFile(filePath: string): Promise<string> {
    const hash = crypto.createHash('sha256');
    const stream = fs.createReadStream(filePath);

    return new Promise((resolve, reject) => {
      stream.on('data', (chunk) => hash.update(chunk));
      stream.on('end', () => resolve(`sha256:${hash.digest('hex')}`));
      stream.on('error', reject);
    });
  }

  /**
   * Create a fingerprint hash from file names + sizes (fallback when no weights file)
   */
  private static async hashDirectoryFingerprint(dirPath: string): Promise<string> {
    const hash = crypto.createHash('sha256');
    const entries = await fs.promises.readdir(dirPath, { withFileTypes: true });

    const fingerprint: string[] = [];
    for (const entry of entries) {
      if (entry.isFile()) {
        const stats = await fs.promises.stat(path.join(dirPath, entry.name));
        fingerprint.push(`${entry.name}:${stats.size}`);
      }
    }

    fingerprint.sort(); // Deterministic ordering
    hash.update(fingerprint.join('\n'));
    return `sha256:${hash.digest('hex')}`;
  }
}
