/**
 * AdapterPublishSchema — Strict schema for HuggingFace adapter metadata
 *
 * This is the CONTRACT for all published Continuum adapters. Once in the wild,
 * this format must be stable. Every adapter/publish call validates against this
 * schema before uploading. Inconsistent metadata = broken search.
 *
 * Tag format: continuum:{key}={value}
 * - Keys are lowercase kebab-case
 * - Values are lowercase kebab-case (no spaces, no underscores, no camelCase)
 * - Numeric values are integers (no decimals)
 * - Boolean values are 'true' or 'false'
 *
 * This file is the SINGLE SOURCE OF TRUTH for the tag vocabulary.
 * If a tag key isn't listed here, it MUST NOT be published.
 */

// ============================================================================
// Tag Keys — the complete vocabulary
// ============================================================================

/** All valid continuum tag keys. Adding a new key is a schema evolution. */
export const CONTINUUM_TAG_KEYS = {
  /** Role the adapter was trained for (e.g., 'sprite-artist', 'backend-engineer') */
  ROLE: 'role',
  /** Skill domain (e.g., 'pixel-art', 'typescript', 'game-physics') */
  SKILL: 'skill',
  /** Base model the adapter was trained on (e.g., 'qwen2.5-coder-14b-instruct') */
  BASE: 'base',
  /** Training score 0-100 (teacher-graded or phenotype validation) */
  SCORE: 'score',
  /** Project type (e.g., 'game-development', 'web-app', 'music-production') */
  PROJECT_TYPE: 'project-type',
  /** Team size (number of students in team project) */
  TEAM_SIZE: 'team-size',
  /** Training epochs */
  EPOCHS: 'epochs',
  /** LoRA rank */
  RANK: 'rank',
  /** Persona name that trained this adapter */
  PERSONA: 'persona',
  /** Academy session mode (knowledge, coding, project, realclasseval, team) */
  MODE: 'mode',
  /** Schema version — increment when tag format changes */
  SCHEMA: 'schema',
} as const;

export type ContinuumTagKey = typeof CONTINUUM_TAG_KEYS[keyof typeof CONTINUUM_TAG_KEYS];

/** Current schema version. Increment on breaking changes to tag format. */
export const CONTINUUM_TAG_SCHEMA_VERSION = 1;

// ============================================================================
// Value Normalization — enforce consistency
// ============================================================================

/**
 * Normalize a tag value to lowercase kebab-case.
 * "Sprite Artist" → "sprite-artist"
 * "sprite_artist" → "sprite-artist"
 * "SpriteArtist" → "sprite-artist"
 * "qwen2.5-coder-14b" → "qwen2.5-coder-14b" (dots preserved for model IDs)
 */
export function normalizeTagValue(value: string): string {
  return value
    // Insert hyphen before uppercase letters (camelCase → kebab-case)
    .replace(/([a-z0-9])([A-Z])/g, '$1-$2')
    // Replace underscores and spaces with hyphens
    .replace(/[_\s]+/g, '-')
    // Lowercase everything
    .toLowerCase()
    // Collapse multiple hyphens
    .replace(/-{2,}/g, '-')
    // Trim leading/trailing hyphens
    .replace(/^-|-$/g, '');
}

/**
 * Normalize a base model name for consistent tagging.
 * Strips org prefix, lowercases, keeps version identifiers.
 * "Qwen/Qwen2.5-Coder-14B-Instruct" → "qwen2.5-coder-14b-instruct"
 * "unsloth/Llama-3.2-3B-Instruct" → "llama-3.2-3b-instruct"
 */
export function normalizeBaseModel(modelId: string): string {
  // Strip org prefix (Qwen/, unsloth/, meta-llama/)
  const name = modelId.includes('/') ? modelId.split('/').pop()! : modelId;
  return normalizeTagValue(name);
}

// ============================================================================
// Tag Builder — from AdapterManifest to validated tags
// ============================================================================

export interface AdapterPublishMetadata {
  role?: string;
  skill?: string;
  baseModel: string;
  score?: number;
  projectType?: string;
  teamSize?: number;
  epochs?: number;
  rank?: number;
  persona?: string;
  mode?: string;
}

/**
 * Build validated continuum:* tags from adapter metadata.
 * Every tag passes through normalization. Invalid values are skipped.
 * Schema version tag is always included.
 */
export function buildContinuumTags(metadata: AdapterPublishMetadata): string[] {
  const tags: string[] = [
    // Standard PEFT/LoRA tags for HF discoverability
    'peft',
    'lora',
    'continuum',
    // Schema version — always present
    `continuum:${CONTINUUM_TAG_KEYS.SCHEMA}=${CONTINUUM_TAG_SCHEMA_VERSION}`,
  ];

  // HF native base model tag (their format, not ours)
  if (metadata.baseModel) {
    tags.push(`base_model:${metadata.baseModel}`);
    tags.push(`continuum:${CONTINUUM_TAG_KEYS.BASE}=${normalizeBaseModel(metadata.baseModel)}`);
  }

  // String tags — normalize values
  if (metadata.role) {
    tags.push(`continuum:${CONTINUUM_TAG_KEYS.ROLE}=${normalizeTagValue(metadata.role)}`);
  }
  if (metadata.skill) {
    tags.push(`continuum:${CONTINUUM_TAG_KEYS.SKILL}=${normalizeTagValue(metadata.skill)}`);
  }
  if (metadata.projectType) {
    tags.push(`continuum:${CONTINUUM_TAG_KEYS.PROJECT_TYPE}=${normalizeTagValue(metadata.projectType)}`);
  }
  if (metadata.persona) {
    tags.push(`continuum:${CONTINUUM_TAG_KEYS.PERSONA}=${normalizeTagValue(metadata.persona)}`);
  }
  if (metadata.mode) {
    tags.push(`continuum:${CONTINUUM_TAG_KEYS.MODE}=${normalizeTagValue(metadata.mode)}`);
  }

  // Numeric tags — integers only
  if (metadata.score !== undefined && metadata.score >= 0 && metadata.score <= 100) {
    tags.push(`continuum:${CONTINUUM_TAG_KEYS.SCORE}=${Math.round(metadata.score)}`);
  }
  if (metadata.teamSize !== undefined && metadata.teamSize > 0) {
    tags.push(`continuum:${CONTINUUM_TAG_KEYS.TEAM_SIZE}=${Math.round(metadata.teamSize)}`);
  }
  if (metadata.epochs !== undefined && metadata.epochs > 0) {
    tags.push(`continuum:${CONTINUUM_TAG_KEYS.EPOCHS}=${Math.round(metadata.epochs)}`);
  }
  if (metadata.rank !== undefined && metadata.rank > 0) {
    tags.push(`continuum:${CONTINUUM_TAG_KEYS.RANK}=${Math.round(metadata.rank)}`);
  }

  return tags;
}

/**
 * Parse a continuum:* tag back into key-value pair.
 * Returns null for non-continuum tags.
 */
export function parseContinuumTag(tag: string): { key: string; value: string } | null {
  if (!tag.startsWith('continuum:')) return null;
  const rest = tag.slice('continuum:'.length);
  const eqIdx = rest.indexOf('=');
  if (eqIdx === -1) return null;
  return {
    key: rest.slice(0, eqIdx),
    value: rest.slice(eqIdx + 1),
  };
}

/**
 * Validate that a set of tags conforms to the schema.
 * Returns errors for any invalid tags.
 */
export function validateTags(tags: string[]): { valid: boolean; errors: string[] } {
  const errors: string[] = [];
  const validKeys = new Set(Object.values(CONTINUUM_TAG_KEYS));

  for (const tag of tags) {
    const parsed = parseContinuumTag(tag);
    if (!parsed) continue; // Skip non-continuum tags

    if (!validKeys.has(parsed.key)) {
      errors.push(`Unknown continuum tag key: '${parsed.key}'. Valid keys: ${[...validKeys].join(', ')}`);
    }
  }

  // Schema version must be present
  const hasSchema = tags.some(t => t.startsWith(`continuum:${CONTINUUM_TAG_KEYS.SCHEMA}=`));
  if (!hasSchema) {
    errors.push('Missing required tag: continuum:schema=N');
  }

  return { valid: errors.length === 0, errors };
}
