/**
 * AdapterPackageTypes — Shared type definitions for adapter packaging
 *
 * These types are environment-agnostic (no Node.js APIs).
 * The AdapterPackage class (server-only) implements operations on these types.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { TrainingMetadata } from '../entities/GenomeLayerEntity';

/**
 * Adapter package manifest — mirrors GenomeLayerEntity fields.
 * Written as manifest.json inside every adapter directory.
 */
export interface AdapterPackageManifest {
  /** Unique identifier for this adapter (becomes GenomeLayerEntity.id) */
  id: UUID;
  /** Human-readable name (e.g., "helper-ai-conversational") */
  name: string;
  /** Trait type label (e.g., "conversational", "teaching") */
  traitType: string;
  /** How this layer was created */
  source: 'trained' | 'refined' | 'downloaded' | 'inherited' | 'system';
  /** Base model used for training (short name or HuggingFace name) */
  baseModel: string;
  /** LoRA rank used during training */
  rank: number;
  /** Adapter directory size in megabytes */
  sizeMB: number;
  /** Persona this adapter was trained for */
  personaId: UUID;
  /** Persona display name */
  personaName: string;
  /** Training metadata for provenance */
  trainingMetadata: TrainingMetadata;
  /** SHA-256 hash of adapter_model.safetensors for integrity verification */
  contentHash?: string;
  /** ISO timestamp of creation */
  createdAt: string;
  /** Manifest format version */
  version: number;
}
