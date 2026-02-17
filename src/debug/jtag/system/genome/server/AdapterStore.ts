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
   * Discover all adapters in the store
   */
  static discoverAll(): DiscoveredAdapter[] {
    const storeDir = AdapterStore.storeRoot;
    if (!fs.existsSync(storeDir)) return [];

    const entries = fs.readdirSync(storeDir, { withFileTypes: true });
    const adapters: DiscoveredAdapter[] = [];

    for (const entry of entries) {
      if (!entry.isDirectory()) continue;

      const dirPath = path.join(storeDir, entry.name);
      const adapter = AdapterStore._readAdapter(dirPath);
      if (adapter) adapters.push(adapter);
    }

    return adapters;
  }

  /**
   * Discover all adapters belonging to a specific persona
   */
  static discoverForPersona(personaId: string): DiscoveredAdapter[] {
    return AdapterStore.discoverAll()
      .filter(a => a.manifest.personaId === personaId);
  }

  /**
   * Get the LATEST adapter for a persona + domain (trait type)
   *
   * When multiple training runs exist for the same domain,
   * returns the most recently created one (by createdAt timestamp).
   */
  static latestForPersonaDomain(personaId: string, domain: string): DiscoveredAdapter | null {
    const matches = AdapterStore.discoverForPersona(personaId)
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
  static latestByDomainForPersona(personaId: string): Map<string, DiscoveredAdapter> {
    const all = AdapterStore.discoverForPersona(personaId)
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
