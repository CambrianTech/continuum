/**
 * AdapterStore — SINGLE SOURCE OF TRUTH for LoRA adapter discovery
 *
 * Scans the adapter filesystem (SystemPaths.genome.adapters) for trained adapters.
 * Each adapter directory contains:
 *   - manifest.json      — metadata (personaId, traitType, baseModel, etc.)
 *   - adapter_config.json — PEFT configuration
 *   - adapter_model.safetensors — weights
 *
 * This replaces all hardcoded adapter paths and JSON configs. If you want to know
 * what adapters exist, ask AdapterStore. Period.
 */

import * as fs from 'fs';
import * as path from 'path';
import { SystemPaths } from '../../core/config/SystemPaths';
import { LOCAL_MODELS } from '../../shared/Constants';
import type { QuantizationInfo } from '../shared/AdapterPackageTypes';

/**
 * Adapter manifest — the metadata written by genome/train to each adapter directory
 */
export interface AdapterManifest {
  id: string;
  name: string;
  traitType: string;
  source: string;
  baseModel: string;
  rank: number;
  sizeMB: number;
  personaId: string;
  personaName: string;
  trainingMetadata?: {
    epochs: number;
    loss: number;
    performance: number;
    trainingDuration: number;
    datasetHash?: string;
  };
  contentHash?: string;
  createdAt: string;
  version: number;
  /** QLoRA quantization metadata — tracks base model quantization during training */
  quantization?: QuantizationInfo;
}

/**
 * A discovered adapter on disk — manifest + validated path
 */
export interface DiscoveredAdapter {
  /** Absolute path to the adapter directory */
  dirPath: string;
  /** Parsed manifest.json */
  manifest: AdapterManifest;
  /** Whether adapter_model.safetensors exists */
  hasWeights: boolean;
}

/**
 * AdapterStore — Filesystem-based adapter registry
 *
 * Usage:
 *   const adapters = AdapterStore.discoverAll();
 *   const mine = AdapterStore.discoverForPersona(personaId);
 *   const latest = AdapterStore.latestForPersonaDomain(personaId, 'conversational');
 */
export class AdapterStore {
  /**
   * The adapter store root directory
   * SINGLE SOURCE OF TRUTH — all other code should use this
   */
  static get storeRoot(): string {
    return SystemPaths.genome.adapters;
  }

  /**
   * Discover all adapters in the store.
   * Also scans legacy $REPO location as a migration bridge.
   */
  static discoverAll(): DiscoveredAdapter[] {
    const adapters = AdapterStore._scanRoot(AdapterStore.storeRoot);

    // MIGRATION: also scan legacy $REPO/.continuum/genome/adapters
    const legacyRoot = path.join(process.cwd(), '.continuum', 'genome', 'adapters');
    if (legacyRoot !== AdapterStore.storeRoot && fs.existsSync(legacyRoot)) {
      const legacy = AdapterStore._scanRoot(legacyRoot);
      const seen = new Set(adapters.map(a => a.manifest.id));
      for (const a of legacy) {
        if (!seen.has(a.manifest.id)) adapters.push(a);
      }
    }

    return adapters;
  }

  /**
   * Discover all adapters belonging to a specific persona.
   * Matches by personaId (UUID) first. If no UUID match, falls back
   * to personaName matching — critical because data reseeding assigns
   * new UUIDs while adapter manifests retain the training-time UUID.
   */
  static discoverForPersona(personaId: string, personaName?: string): DiscoveredAdapter[] {
    const all = AdapterStore.discoverAll();

    // Primary: exact UUID match
    const byId = all.filter(a => a.manifest.personaId === personaId);
    if (byId.length > 0) return byId;

    // Fallback: match by personaName (survives reseed)
    if (personaName) {
      return all.filter(a => a.manifest.personaName === personaName);
    }

    return [];
  }

  /**
   * Get the LATEST adapter for a persona + domain (trait type)
   *
   * When multiple training runs exist for the same domain,
   * returns the most recently created one (by createdAt timestamp).
   */
  static latestForPersonaDomain(personaId: string, domain: string, personaName?: string): DiscoveredAdapter | null {
    const matches = AdapterStore.discoverForPersona(personaId, personaName)
      .filter(a => a.manifest.traitType === domain && a.hasWeights)
      .sort((a, b) => {
        // Sort descending by creation time (newest first)
        const timeA = new Date(a.manifest.createdAt).getTime();
        const timeB = new Date(b.manifest.createdAt).getTime();
        return timeB - timeA;
      });

    return matches[0] ?? null;
  }

  /**
   * Get the latest adapter for each domain for a persona
   *
   * Returns a Map of domain → DiscoveredAdapter (most recent per domain).
   * This is what PersonaGenome should register as initial adapters.
   */
  static latestByDomainForPersona(personaId: string, personaName?: string): Map<string, DiscoveredAdapter> {
    const all = AdapterStore.discoverForPersona(personaId, personaName)
      .filter(a => a.hasWeights);

    const byDomain = new Map<string, DiscoveredAdapter>();

    for (const adapter of all) {
      const domain = adapter.manifest.traitType;
      const existing = byDomain.get(domain);

      if (!existing) {
        byDomain.set(domain, adapter);
      } else {
        // Keep the most recent
        const existingTime = new Date(existing.manifest.createdAt).getTime();
        const newTime = new Date(adapter.manifest.createdAt).getTime();
        if (newTime > existingTime) {
          byDomain.set(domain, adapter);
        }
      }
    }

    return byDomain;
  }

  /**
   * Normalize a model name to its canonical HuggingFace ID
   *
   * Handles short names ("smollm2:135m"), bare names ("llama3.2"),
   * and full HuggingFace IDs ("unsloth/Llama-3.2-3B-Instruct").
   * Returns lowercase for consistent comparison.
   */
  static normalizeModelName(modelName: string): string {
    return LOCAL_MODELS.mapToHuggingFace(modelName).toLowerCase();
  }

  /**
   * Check if an adapter is compatible with a given inference model
   *
   * LoRA adapters are architecture-specific — an adapter trained on SmolLM2
   * CANNOT be applied to Llama-3.2. The tensor shapes won't match.
   */
  static isCompatibleWithModel(adapter: DiscoveredAdapter, inferenceModel: string): boolean {
    const adapterBase = AdapterStore.normalizeModelName(adapter.manifest.baseModel);
    const inferenceBase = AdapterStore.normalizeModelName(inferenceModel);
    return adapterBase === inferenceBase;
  }

  /**
   * Discover adapters for a persona, filtered by model compatibility
   *
   * This is the primary method for production use — returns only adapters
   * that can actually be applied to the current inference model.
   */
  static discoverCompatible(personaId: string, inferenceModel: string, personaName?: string): DiscoveredAdapter[] {
    return AdapterStore.discoverForPersona(personaId, personaName)
      .filter(a => a.hasWeights && AdapterStore.isCompatibleWithModel(a, inferenceModel));
  }

  /**
   * Get latest compatible adapter per domain for a persona
   *
   * Like latestByDomainForPersona but filtered to only adapters
   * matching the inference model architecture.
   */
  static latestCompatibleByDomain(personaId: string, inferenceModel: string, personaName?: string): Map<string, DiscoveredAdapter> {
    const compatible = AdapterStore.discoverCompatible(personaId, inferenceModel, personaName);
    const byDomain = new Map<string, DiscoveredAdapter>();

    for (const adapter of compatible) {
      const domain = adapter.manifest.traitType;
      const existing = byDomain.get(domain);

      if (!existing) {
        byDomain.set(domain, adapter);
      } else {
        const existingTime = new Date(existing.manifest.createdAt).getTime();
        const newTime = new Date(adapter.manifest.createdAt).getTime();
        if (newTime > existingTime) {
          byDomain.set(domain, adapter);
        }
      }
    }

    return byDomain;
  }

  /**
   * Validate that an adapter path is a real, usable adapter on disk
   *
   * Checks for:
   *   - Directory exists
   *   - Contains adapter_model.safetensors
   *   - Contains adapter_config.json
   */
  static isValidAdapterPath(adapterPath: string): boolean {
    if (!fs.existsSync(adapterPath)) return false;

    const stat = fs.statSync(adapterPath);
    if (stat.isDirectory()) {
      return fs.existsSync(path.join(adapterPath, 'adapter_model.safetensors'));
    }

    // Direct .safetensors file
    if (adapterPath.endsWith('.safetensors')) {
      return fs.existsSync(adapterPath);
    }

    return false;
  }

  /**
   * Scan a single root directory for adapter subdirectories
   */
  private static _scanRoot(rootDir: string): DiscoveredAdapter[] {
    if (!fs.existsSync(rootDir)) return [];

    const entries = fs.readdirSync(rootDir, { withFileTypes: true });
    const adapters: DiscoveredAdapter[] = [];

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;
      const dirPath = path.join(rootDir, entry.name);
      const adapter = AdapterStore._readAdapter(dirPath);
      if (adapter) adapters.push(adapter);
    }

    return adapters;
  }

  /**
   * Read a single adapter directory, returning null if invalid
   */
  private static _readAdapter(dirPath: string): DiscoveredAdapter | null {
    const manifestPath = path.join(dirPath, 'manifest.json');
    if (!fs.existsSync(manifestPath)) return null;

    try {
      const raw = fs.readFileSync(manifestPath, 'utf-8');
      const manifest: AdapterManifest = JSON.parse(raw);

      const hasWeights = fs.existsSync(
        path.join(dirPath, 'adapter_model.safetensors')
      );

      return { dirPath, manifest, hasWeights };
    } catch {
      // Corrupted manifest — skip silently
      return null;
    }
  }
}
