/**
 * AdapterPackage - Standardized packaging for trained LoRA adapters
 *
 * Handles:
 * - Writing/reading manifest.json files in adapter directories
 * - Calculating directory size and content hashes
 * - Converting manifests to GenomeLayerEntity instances
 * - Scanning adapter directories for existing packages
 * - Distribution: pack/unpack/import .genome.tgz archives
 *
 * Each adapter directory follows a standard layout:
 *   .continuum/genome/adapters/{name}-{timestamp}/
 *   ├── manifest.json                    ← Package metadata
 *   ├── adapter_model.safetensors        ← PEFT weights
 *   ├── adapter_config.json              ← LoRA config
 *   └── ...                              ← Other PEFT output files
 *
 * Distribution format (.genome.tgz):
 *   Strips training artifacts (~400MB checkpoints, optimizer state),
 *   includes only inference-essential files, verifies integrity on import.
 *
 * SERVER-ONLY: Uses Node.js fs, path, crypto, child_process APIs
 */

import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import { execFile } from 'child_process';
import { promisify } from 'util';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { generateUUID } from '../../core/types/CrossPlatformUUID';
import { GenomeLayerEntity } from '../entities/GenomeLayerEntity';
import type { TrainingMetadata } from '../entities/GenomeLayerEntity';
import type { AdapterPackageManifest, QuantizationInfo, PackResult, ImportResult } from '../shared/AdapterPackageTypes';
import { DISTRIBUTABLE_FILES } from '../shared/AdapterPackageTypes';
import { SystemPaths } from '../../core/config/SystemPaths';
import { DataCreate } from '../../../commands/data/create/shared/DataCreateTypes';
import { EmbeddingGenerate } from '../../../commands/ai/embedding/generate/shared/EmbeddingGenerateTypes';
import { AdapterStore } from './AdapterStore';

// Re-export for convenience
export type { AdapterPackageManifest, QuantizationInfo, PackResult, ImportResult } from '../shared/AdapterPackageTypes';

const execFileAsync = promisify(execFile);

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
    entity.quantization = manifest.quantization;
    entity.contentHash = manifest.contentHash;
    entity.tags = [manifest.traitType, manifest.baseModel, manifest.personaName.toLowerCase()];
    entity.generation = 0;

    const createdAt = new Date(manifest.createdAt);
    entity.createdAt = createdAt;
    entity.updatedAt = createdAt;

    return entity;
  }

  /**
   * Generate a capability embedding for a GenomeLayerEntity.
   *
   * The embedding encodes WHAT the adapter can do, not just its name/description.
   * When exam questions are provided, they're included in the embedding text —
   * meaning the vector represents actual tested competence. A biology adapter's
   * embedding naturally overlaps biochem because the exam questions about cellular
   * processes, molecular interactions, chemical pathways all embed in that neighborhood.
   *
   * Geometry of competence, not keywords.
   */
  static async generateLayerEmbedding(
    entity: GenomeLayerEntity,
    examQuestions?: string[]
  ): Promise<void> {
    const parts = [entity.name, entity.description, `domain: ${entity.traitType}`];
    if (entity.tags.length > 0) {
      parts.push(`tags: ${entity.tags.join(', ')}`);
    }
    if (examQuestions?.length) {
      parts.push('proven competence:', ...examQuestions);
    }

    const text = parts.join('\n');
    const result = await EmbeddingGenerate.execute({ input: text });

    if (result.success && result.embeddings.length > 0) {
      entity.embedding = result.embeddings[0];
      entity.embeddingDimension = result.dimensions;
    }
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
    quantization?: QuantizationInfo;
  }): AdapterPackageManifest {
    const safeName = params.personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-');

    const manifest: AdapterPackageManifest = {
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

    if (params.quantization) {
      manifest.quantization = params.quantization;
    }

    return manifest;
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

  // ==================== Distribution (pack / unpack / import) ====================

  /**
   * Pack an adapter directory into a .genome.tgz distribution archive.
   *
   * Only includes inference-essential files (DISTRIBUTABLE_FILES), stripping
   * training artifacts like checkpoint dirs (~400MB), optimizer.pt (~389MB), etc.
   * Result: ~200MB archive vs ~600MB source directory.
   *
   * Archive naming: {manifest.name}-{manifest.id.slice(0,8)}.genome.tgz
   */
  static async pack(adapterPath: string, outputDir?: string): Promise<PackResult> {
    const manifest = await this.readManifest(adapterPath);

    // Collect only distributable files that actually exist in this adapter
    const filesToInclude: string[] = [];
    for (const file of DISTRIBUTABLE_FILES) {
      if (fs.existsSync(path.join(adapterPath, file))) {
        filesToInclude.push(file);
      }
    }

    if (filesToInclude.length === 0) {
      throw new Error(`No distributable files found in ${adapterPath}`);
    }

    // Ensure manifest.json is always included (it's in DISTRIBUTABLE_FILES but be explicit)
    if (!filesToInclude.includes('manifest.json')) {
      throw new Error(`manifest.json missing from ${adapterPath}`);
    }

    // Ensure output directory exists
    const targetDir = outputDir ?? SystemPaths.genome.packages;
    await fs.promises.mkdir(targetDir, { recursive: true });

    // Archive name: {name}-{shortId}.genome.tgz
    const shortId = manifest.id.slice(0, 8);
    const archiveName = `${manifest.name}-${shortId}.genome.tgz`;
    const tgzPath = path.join(targetDir, archiveName);

    // Create .tgz via system tar (available on macOS + Linux)
    await execFileAsync('tar', ['czf', tgzPath, '-C', adapterPath, ...filesToInclude]);

    // Calculate archive size
    const archiveStats = await fs.promises.stat(tgzPath);
    const packageSizeMB = Math.round((archiveStats.size / (1024 * 1024)) * 100) / 100;

    return {
      tgzPath,
      manifest,
      contentHash: manifest.contentHash ?? '',
      packageSizeMB,
      filesIncluded: filesToInclude,
    };
  }

  /**
   * Unpack a .genome.tgz archive and verify integrity.
   *
   * Extracts to a temp directory, verifies SHA-256 of adapter_model.safetensors
   * against manifest.contentHash, then moves to final destination.
   *
   * @throws Error if content hash verification fails (archive is corrupt or tampered)
   */
  static async unpack(tgzPath: string, targetDir?: string): Promise<ImportResult> {
    // Create temp extraction directory
    const tempDir = path.join(SystemPaths.temp.root, `genome-unpack-${Date.now()}`);
    await fs.promises.mkdir(tempDir, { recursive: true });

    try {
      // Extract archive
      await execFileAsync('tar', ['xzf', tgzPath, '-C', tempDir]);

      // Read manifest from extracted files
      const manifest = await this.readManifest(tempDir);

      // Verify integrity: SHA-256 of adapter_model.safetensors must match manifest.contentHash
      let contentHashVerified = false;
      const weightsPath = path.join(tempDir, 'adapter_model.safetensors');

      if (manifest.contentHash && fs.existsSync(weightsPath)) {
        const actualHash = await this.hashFile(weightsPath);
        if (actualHash !== manifest.contentHash) {
          // Clean up before throwing — don't leave corrupt files around
          await fs.promises.rm(tempDir, { recursive: true, force: true });
          throw new Error(
            `Content hash mismatch: expected ${manifest.contentHash}, got ${actualHash}. ` +
            `Archive may be corrupt or tampered.`
          );
        }
        contentHashVerified = true;
      }

      // Move to final destination
      const baseDir = targetDir ?? AdapterStore.storeRoot;
      await fs.promises.mkdir(baseDir, { recursive: true });
      const finalDir = path.join(baseDir, `${manifest.name}-${Date.now()}`);
      await fs.promises.rename(tempDir, finalDir);

      return {
        adapterPath: finalDir,
        manifest,
        contentHashVerified,
      };
    } catch (error) {
      // Clean up temp dir on any failure (might already be gone if hash check cleaned it)
      if (fs.existsSync(tempDir)) {
        await fs.promises.rm(tempDir, { recursive: true, force: true });
      }
      throw error;
    }
  }

  /**
   * Import a .genome.tgz archive: unpack, verify, register in database.
   *
   * Higher-level pipeline that combines unpack() with ORM registration.
   * After import, AdapterStore.discoverAll() will find the adapter automatically.
   *
   * @param opts.inferenceModel - If provided, verifies model compatibility before registering
   */
  static async importAdapter(
    tgzPath: string,
    opts?: { inferenceModel?: string }
  ): Promise<ImportResult> {
    const result = await this.unpack(tgzPath);

    // Optional model compatibility check
    if (opts?.inferenceModel) {
      const normalizedAdapter = AdapterStore.normalizeModelName(result.manifest.baseModel);
      const normalizedTarget = AdapterStore.normalizeModelName(opts.inferenceModel);
      if (normalizedAdapter !== normalizedTarget) {
        // Clean up extracted files — incompatible adapter is useless
        await fs.promises.rm(result.adapterPath, { recursive: true, force: true });
        throw new Error(
          `Model incompatible: adapter trained on ${result.manifest.baseModel} ` +
          `(${normalizedAdapter}), target is ${opts.inferenceModel} (${normalizedTarget})`
        );
      }
    }

    // Create GenomeLayerEntity and persist to database
    const entity = this.toGenomeLayerEntity(result.manifest, result.adapterPath);
    await DataCreate.execute({
      collection: GenomeLayerEntity.collection,
      data: entity,
      dbHandle: 'default',
    });

    return result;
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
