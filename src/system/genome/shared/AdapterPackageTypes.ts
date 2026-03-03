/**
 * AdapterPackageTypes — Shared type definitions for adapter packaging
 *
 * These types are environment-agnostic (no Node.js APIs).
 * The AdapterPackage class (server-only) implements operations on these types.
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { TrainingMetadata } from '../entities/GenomeLayerEntity';

/**
 * Files included in a .genome.tgz distribution archive (inference-essential only).
 *
 * Excludes training artifacts: checkpoint dirs (~400MB), optimizer.pt (~389MB),
 * scheduler.pt, training_args.bin, rng_state.pth, README.md, merges.txt, vocab.json.
 * Result: ~200MB package vs ~600MB source directory.
 */
export const DISTRIBUTABLE_FILES = [
  'manifest.json',
  'adapter_model.safetensors',
  'adapter_config.json',
  'tokenizer.json',
  'tokenizer_config.json',
  'special_tokens_map.json',
  'chat_template.jinja',
] as const;

/** Result of packing an adapter directory into a .genome.tgz archive */
export interface PackResult {
  /** Absolute path to the created .genome.tgz file */
  tgzPath: string;
  /** The adapter manifest included in the archive */
  manifest: AdapterPackageManifest;
  /** SHA-256 content hash from the manifest (for integrity verification on import) */
  contentHash: string;
  /** Archive size in megabytes */
  packageSizeMB: number;
  /** List of files included in the archive */
  filesIncluded: string[];
}

/** Result of importing (unpacking + registering) a .genome.tgz archive */
export interface ImportResult {
  /** Absolute path to the extracted adapter directory */
  adapterPath: string;
  /** The adapter manifest from the archive */
  manifest: AdapterPackageManifest;
  /** Whether the SHA-256 hash was verified against manifest.contentHash */
  contentHashVerified: boolean;
}

/**
 * Quantization metadata — tracks whether QLoRA was used during training.
 *
 * Key insight: QLoRA quantizes the BASE MODEL during training, but the
 * LoRA adapter weights are always full precision (FP16/BF16 safetensors).
 * The same adapter works on both quantized and non-quantized inference paths.
 * This metadata tells us HOW the adapter was trained, not what format it is.
 */
export interface QuantizationInfo {
  /** Was the base model quantized during training? */
  enabled: boolean;
  /** Quantization precision (4-bit or 8-bit) */
  bits: 4 | 8;
  /** Quantization algorithm used */
  type: 'nf4' | 'int4' | 'int8' | 'fp8';
  /** BitsAndBytes double quantization (reduces memory further) */
  doubleQuant: boolean;
  /** Compute dtype used during training */
  computeDtype: 'bfloat16' | 'float16';
}

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
  /** QLoRA quantization metadata — tracks base model quantization during training */
  quantization?: QuantizationInfo;
}
