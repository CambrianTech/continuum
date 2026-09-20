/**
 * ForgeArtifact Entity — foundry-generated output for a recipe.
 *
 * Persists a `ForgeArtifact` (Rust source of truth at
 * `src/../core/continuum-core/src/forge/artifact.rs`, ts-rs generated
 * type at `shared/generated/forge/ForgeArtifact.ts`) into the Continuum
 * data layer. Phase 3 of continuum#1164.
 *
 * # Why both recipe + artifact get entities
 *
 * The artifact carries a SNAPSHOT of the recipe fields at run time
 * (denormalized so the artifact card renders without re-fetching the
 * recipe). The artifact also carries execution outputs only the foundry
 * knows. Recipe lineage is via `recipeId` + `recipeVersion` (frozen at
 * run time so a later recipe edit can't retroactively rewrite what
 * this artifact claims to come from).
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { BaseEntity } from './BaseEntity';
import { TextField, JsonField, NumberField, ForeignKeyField, TEXT_LENGTH } from '../decorators/FieldDecorators';
import type {
  AlloyHardware,
  AlloySource,
  BenchmarkDef,
  CorpusRef,
  HardwareProfile,
  PriorBaseline,
  QuantTier,
} from '@shared/generated/forge';

export class ForgeArtifactEntity extends BaseEntity {
  static readonly collection = 'forge_artifacts';

  get collection(): string {
    return ForgeArtifactEntity.collection;
  }

  // === Recipe lineage (frozen at run time) ===

  @ForeignKeyField({ references: 'forge_recipes', index: true })
  recipeId!: UUID;

  /**
   * Recipe version at run time (semver). Pinned so a later recipe
   * revision doesn't retroactively change what this artifact claims
   * to come from.
   */
  @TextField({ maxLength: TEXT_LENGTH.SHORT })
  recipeVersion!: string;

  /** Recipe `name` snapshot — denormalized for card-render efficiency. */
  @TextField({ maxLength: TEXT_LENGTH.DEFAULT, index: true })
  recipeName!: string;

  // === Snapshot of recipe authored fields ===

  @TextField({ maxLength: TEXT_LENGTH.LONG })
  description!: string;

  @TextField({ maxLength: TEXT_LENGTH.DEFAULT })
  userSummary!: string;

  @TextField({ maxLength: TEXT_LENGTH.DEFAULT, index: true })
  author!: string;

  @JsonField()
  tags!: string[];

  @TextField({ maxLength: TEXT_LENGTH.SHORT })
  license!: string;

  @TextField({ maxLength: TEXT_LENGTH.LONG, nullable: true })
  methodologyPaperUrl?: string;

  @JsonField()
  limitations!: string[];

  @JsonField()
  priorMetricBaselines!: PriorBaseline[];

  @JsonField()
  source!: AlloySource;

  @JsonField()
  calibrationCorpus!: CorpusRef;

  @JsonField()
  quantTiers!: QuantTier[];

  @JsonField()
  evaluationBenchmarks!: BenchmarkDef[];

  @JsonField()
  hardware!: AlloyHardware;

  // === Execution outputs (only the foundry knows these) ===

  @NumberField({ summary: true })
  forgedAtMs!: number;

  @NumberField({ nullable: true })
  durationMinutes?: number;

  @NumberField({ nullable: true, summary: true })
  forgedParamsB?: number;

  @NumberField({ nullable: true })
  activeParamsB?: number;

  @JsonField()
  hardwareVerified!: HardwareProfile[];

  /**
   * Content-addressable hash of the populated artifact JSON. Used as
   * the verification anchor by publish_model.py and by the proof-
   * contract trust layer (see grid/FORGE-ALLOY-PROOF-CONTRACTS.md).
   * Format: "sha256:<hex>" matching admission's content_hash convention.
   */
  @TextField({ maxLength: TEXT_LENGTH.DEFAULT, nullable: true, index: true, unique: true })
  alloyHash?: string;

  /**
   * Full execution results blob. v1 carries this as opaque JSON
   * matching the existing Python AlloyResults shape. Phase 2 of #1164
   * types this as a first-class Rust struct once the foundry executor
   * needs it.
   */
  @JsonField({ nullable: true })
  results?: unknown;

  /** Publication receipt blob. Phase 2 typing same as `results`. */
  @JsonField({ nullable: true })
  receipt?: unknown;

  /** Integrity attestation blob. Phase 2 typing same as `results`. */
  @JsonField({ nullable: true })
  integrity?: unknown;

  /** Required by BaseEntity. v1: minimal validation. */
  validate(): { success: boolean; error?: string } {
    if (!this.recipeId) {
      return { success: false, error: 'ForgeArtifact.recipeId must be set (lineage)' };
    }
    if (!this.recipeVersion || this.recipeVersion.trim().length === 0) {
      return { success: false, error: 'ForgeArtifact.recipeVersion must be non-empty (snapshot)' };
    }
    if (!this.recipeName || this.recipeName.trim().length === 0) {
      return { success: false, error: 'ForgeArtifact.recipeName must be non-empty (snapshot)' };
    }
    if (!this.forgedAtMs || this.forgedAtMs <= 0) {
      return { success: false, error: 'ForgeArtifact.forgedAtMs must be set (foundry start time)' };
    }
    return { success: true };
  }
}
