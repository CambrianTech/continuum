/**
 * LayerLoader - Load LoRA layers from disk
 * ==========================================
 *
 * Responsible for loading LoRA adapter layers from the filesystem.
 * Layers are stored in: .continuum/genomes/layers/{layerId}/
 *
 * File structure per layer:
 * - adapter_config.json: LoRA configuration
 * - metadata.json: Layer metadata
 * - adapter_model.safetensors: Actual weights (or .bin, .gguf)
 *
 * Phase 2.2: Start with JSON mock files, add real LoRA support later
 */

import { promises as fs } from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';
import type { UUID } from '../../../system/core/types/JTAGTypes';
import type {
  LoadedLayer,
  LoRAConfig,
  LayerMetadata,
  LoaderOptions,
  LoaderStats,
  ValidationResult,
} from '../shared/GenomeAssemblyTypes';
import { GenomeAssemblyError, AssemblyErrorCode } from '../shared/GenomeAssemblyTypes';

/**
 * LayerLoader - Load LoRA layers from disk
 */
export class LayerLoader {
  private basePath: string;
  private stats: LoaderStats;

  constructor(basePath?: string) {
    // Default to .continuum/genomes/layers/ in project root
    this.basePath = basePath || path.join(process.cwd(), '.continuum', 'genomes', 'layers');

    this.stats = {
      layersLoaded: 0,
      bytesRead: 0,
      avgLoadTimeMs: 0,
      failures: 0,
      checksumFailures: 0,
    };

    console.log(`📦 LayerLoader initialized: ${this.basePath}`);
  }

  /**
   * Load a complete LoRA layer by ID
   * @param layerId UUID of layer to load
   * @param options Loading options
   * @returns Loaded layer with config, metadata, and weights
   */
  async loadLayer(layerId: UUID, options: LoaderOptions = {}): Promise<LoadedLayer> {
    const startTime = Date.now();

    try {
      console.log(`📥 Loading layer: ${layerId}`);

      // Build layer directory path
      const layerDir = path.join(this.basePath, layerId);

      // Check if layer directory exists
      const exists = await this.layerExists(layerId);
      if (!exists) {
        throw new GenomeAssemblyError(
          `Layer not found: ${layerId}`,
          AssemblyErrorCode.LAYER_NOT_FOUND,
          layerId
        );
      }

      // Load metadata first
      const metadata = await this.loadMetadata(layerId);

      // Load LoRA config
      const config = await this.loadConfig(layerId);

      // Load weights (unless metadataOnly)
      let weights: Buffer;
      let sizeBytes: number;

      if (options.metadataOnly) {
        console.log(`⏭️  Skipping weights (metadata-only mode)`);
        weights = Buffer.alloc(0);
        sizeBytes = 0;
      } else {
        const weightsResult = await this.loadWeights(layerId, metadata.format);
        weights = weightsResult.buffer;
        sizeBytes = weightsResult.size;

        // Validate checksum if requested
        if (options.validateChecksum) {
          const valid = await this.validateChecksum(weights, metadata.checksum);
          if (!valid) {
            this.stats.checksumFailures++;
            throw new GenomeAssemblyError(
              `Checksum mismatch for layer: ${layerId}`,
              AssemblyErrorCode.CHECKSUM_MISMATCH,
              layerId
            );
          }
          console.log(`✅ Checksum validated`);
        }
      }

      const loadTimeMs = Date.now() - startTime;

      // Create loaded layer object
      const loadedLayer: LoadedLayer = {
        layerId,
        config,
        metadata,
        weights,
        sizeBytes,
        format: metadata.format,
        loadedAt: Date.now(),
      };

      // Update statistics
      this.stats.layersLoaded++;
      this.stats.bytesRead += sizeBytes;
      this.stats.avgLoadTimeMs =
        (this.stats.avgLoadTimeMs * (this.stats.layersLoaded - 1) + loadTimeMs) /
        this.stats.layersLoaded;

      console.log(`✅ Layer loaded: ${layerId} (${sizeBytes} bytes, ${loadTimeMs}ms)`);

      return loadedLayer;
    } catch (error) {
      this.stats.failures++;

      if (error instanceof GenomeAssemblyError) {
        throw error;
      }

      throw new GenomeAssemblyError(
        `Failed to load layer: ${error instanceof Error ? error.message : String(error)}`,
        AssemblyErrorCode.LAYER_NOT_FOUND,
        layerId
      );
    }
  }

  /**
   * Check if a layer exists on disk
   * @param layerId UUID of layer
   * @returns True if layer directory exists
   */
  async layerExists(layerId: UUID): Promise<boolean> {
    try {
      const layerDir = path.join(this.basePath, layerId);
      const stats = await fs.stat(layerDir);
      return stats.isDirectory();
    } catch {
      return false;
    }
  }

  /**
   * Get layer metadata without loading weights
   * @param layerId UUID of layer
   * @returns Layer metadata
   */
  async getLayerMetadata(layerId: UUID): Promise<LayerMetadata> {
    return this.loadMetadata(layerId);
  }

  /**
   * Validate a layer's structure and configuration
   * @param layerId UUID of layer
   * @returns Validation result with errors/warnings
   */
  async validateLayer(layerId: UUID): Promise<ValidationResult> {
    const errors: string[] = [];
    const warnings: string[] = [];

    try {
      // Check layer exists
      const exists = await this.layerExists(layerId);
      if (!exists) {
        errors.push(`Layer directory not found: ${layerId}`);
        return { valid: false, errors, warnings };
      }

      // Validate metadata exists
      try {
        await this.loadMetadata(layerId);
      } catch (error) {
        errors.push(`Invalid or missing metadata.json: ${error instanceof Error ? error.message : String(error)}`);
      }

      // Validate config exists
      try {
        await this.loadConfig(layerId);
      } catch (error) {
        errors.push(`Invalid or missing adapter_config.json: ${error instanceof Error ? error.message : String(error)}`);
      }

      // Check for weights file
      const layerDir = path.join(this.basePath, layerId);
      const files = await fs.readdir(layerDir);

      const hasWeights = files.some((f) =>
        f.endsWith('.safetensors') || f.endsWith('.bin') || f.endsWith('.gguf')
      );

      if (!hasWeights) {
        warnings.push('No weights file found (expected .safetensors, .bin, or .gguf)');
      }

      return {
        valid: errors.length === 0,
        errors,
        warnings,
      };
    } catch (error) {
      errors.push(`Validation failed: ${error instanceof Error ? error.message : String(error)}`);
      return { valid: false, errors, warnings };
    }
  }

  /**
   * Get loader statistics
   * @returns Current loader stats
   */
  getStats(): LoaderStats {
    return { ...this.stats };
  }

  /**
   * Reset loader statistics
   */
  resetStats(): void {
    this.stats = {
      layersLoaded: 0,
      bytesRead: 0,
      avgLoadTimeMs: 0,
      failures: 0,
      checksumFailures: 0,
    };
  }

  // ============================================================================
  // Private Helper Methods
  // ============================================================================

  /**
   * Load metadata.json for a layer
   */
  private async loadMetadata(layerId: UUID): Promise<LayerMetadata> {
    const metadataPath = path.join(this.basePath, layerId, 'metadata.json');

    try {
      const content = await fs.readFile(metadataPath, 'utf-8');
      const metadata = JSON.parse(content) as LayerMetadata;

      // Validate required fields
      if (!metadata.layerId || !metadata.name || !metadata.baseModel) {
        throw new Error('Missing required metadata fields (layerId, name, baseModel)');
      }

      return metadata;
    } catch (error) {
      throw new Error(`Failed to load metadata: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /**
   * Load adapter_config.json for a layer
   */
  private async loadConfig(layerId: UUID): Promise<LoRAConfig> {
    const configPath = path.join(this.basePath, layerId, 'adapter_config.json');

    try {
      const content = await fs.readFile(configPath, 'utf-8');
      const config = JSON.parse(content) as LoRAConfig;

      // Validate required fields
      if (typeof config.r !== 'number' || typeof config.lora_alpha !== 'number') {
        throw new Error('Invalid LoRA config: missing r or lora_alpha');
      }

      if (!Array.isArray(config.target_modules) || config.target_modules.length === 0) {
        throw new Error('Invalid LoRA config: target_modules must be non-empty array');
      }

      return config;
    } catch (error) {
      throw new Error(`Failed to load config: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /**
   * Load weights file (safetensors, pytorch, or gguf)
   */
  private async loadWeights(
    layerId: UUID,
    format: 'safetensors' | 'pytorch' | 'gguf'
  ): Promise<{ buffer: Buffer; size: number }> {
    const layerDir = path.join(this.basePath, layerId);

    // Determine weights filename based on format
    const weightsFilename =
      format === 'safetensors'
        ? 'adapter_model.safetensors'
        : format === 'pytorch'
        ? 'adapter_model.bin'
        : 'adapter_model.gguf';

    const weightsPath = path.join(layerDir, weightsFilename);

    try {
      const buffer = await fs.readFile(weightsPath);
      return {
        buffer,
        size: buffer.length,
      };
    } catch (error) {
      throw new Error(`Failed to load weights: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  /**
   * Validate checksum of loaded weights
   */
  private async validateChecksum(buffer: Buffer, expectedChecksum: string): Promise<boolean> {
    // Extract algorithm and hash (e.g., "sha256:abc123...")
    const [algorithm, expectedHash] = expectedChecksum.split(':');

    if (!algorithm || !expectedHash) {
      console.warn(`⚠️  Invalid checksum format: ${expectedChecksum}`);
      return true; // Skip validation if format is wrong
    }

    // Compute actual hash
    const hash = crypto.createHash(algorithm).update(buffer).digest('hex');

    return hash === expectedHash;
  }
}
