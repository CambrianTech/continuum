/**
 * ForgeRecipe Entity — authored input for the foundry pipeline.
 *
 * Persists a `ForgeRecipe` (Rust source of truth at
 * `src/workers/continuum-core/src/forge/recipe.rs`, ts-rs generated
 * type at `shared/generated/forge/ForgeRecipe.ts`) into the Continuum
 * data layer so callers can CRUD recipes via standard `data/*`
 * commands. Phase 3 of continuum#1164 (design at
 * `docs/architecture/FORGE-RECIPE-AS-ENTITY.md`).
 *
 * # Field shape
 *
 * Field declarations mirror the Rust struct one-to-one. The Rust
 * `#[derive(TS)]` is the source of truth for the JSON shape on the
 * wire; this class registers SQL schema metadata for the data daemon's
 * sqlite/postgres adapter. Drift between the two is a known
 * tech-debt cost (see Phase 3 follow-up: auto-derive entity decorators
 * from ts-rs metadata).
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { BaseEntity } from './BaseEntity';
import { TextField, JsonField, NumberField, TEXT_LENGTH } from '../decorators/FieldDecorators';
import type {
  AlloyHardware,
  AlloySource,
  BenchmarkDef,
  CorpusRef,
  PriorBaseline,
  QuantTier,
} from '@shared/generated/forge';

export class ForgeRecipeEntity extends BaseEntity {
  static readonly collection = 'forge_recipes';

  get collection(): string {
    return ForgeRecipeEntity.collection;
  }

  // === Identity ===

  @TextField({ maxLength: TEXT_LENGTH.DEFAULT, index: true, unique: true })
  name!: string;

  /**
   * Recipe semver. Named `recipeVersion` (not `version`) to avoid
   * collision with BaseEntity's row-version `version: number` (ORM
   * optimistic-concurrency anchor). The Rust source-of-truth field
   * is `version: string`; callers populating this entity must map
   * `recipe.version -> recipeVersion`. Phase 2+ may rename the Rust
   * field too for cross-layer alignment.
   */
  @TextField({ maxLength: TEXT_LENGTH.SHORT })
  recipeVersion!: string;

  @TextField({ maxLength: TEXT_LENGTH.LONG })
  description!: string;

  /** One-line plain-English headline. */
  @TextField({ maxLength: TEXT_LENGTH.DEFAULT })
  userSummary!: string;

  @TextField({ maxLength: TEXT_LENGTH.DEFAULT, index: true })
  author!: string;

  @JsonField()
  tags!: string[];

  @TextField({ maxLength: TEXT_LENGTH.SHORT })
  license!: string;

  // === Methodology / falsifiability prose ===

  @TextField({ maxLength: TEXT_LENGTH.LONG, nullable: true })
  methodologyPaperUrl?: string;

  @JsonField()
  limitations!: string[];

  @JsonField()
  priorMetricBaselines!: PriorBaseline[];

  // === Source ===

  @JsonField()
  source!: AlloySource;

  // === Pipeline ===

  /**
   * Stages as opaque JSON values matching the existing AlloyStage
   * discriminated union from forge-alloy/python/forge_alloy/types.py.
   * Phase 2 of #1164 replaces this with a typed RecipeStage enum (Rust
   * side); the JSON shape is unchanged when that lands.
   */
  @JsonField()
  stages!: unknown[];

  @NumberField({ default: 1 })
  cycles!: number;

  // === Calibration / eval inputs ===

  @JsonField()
  calibrationCorpus!: CorpusRef;

  @JsonField()
  quantTiers!: QuantTier[];

  @JsonField()
  evaluationBenchmarks!: BenchmarkDef[];

  // === Hardware target ===

  @JsonField()
  hardware!: AlloyHardware;

  // === Lineage ===

  /**
   * Parent recipe id, if this recipe was forked from another. v1
   * lineage is one-directional (recipe -> recipe); bidirectional
   * lineage (recipe <- artifact) is a future `parentArtifactIds` field
   * per consensus position #9 on continuum#1165.
   */
  @TextField({ maxLength: TEXT_LENGTH.SHORT, nullable: true, index: true })
  parentRecipeId?: UUID;

  // === Timestamps ===

  /**
   * Epoch milliseconds UTC. Same convention as Engram.admittedAtMs from
   * the engram thread (#1129). Stored as @NumberField (sqlite INTEGER /
   * postgres BIGINT) for direct ordering in `data/list orderBy`.
   */
  @NumberField()
  authoredAtMs!: number;

  @NumberField()
  updatedAtMs!: number;

  /** Required by BaseEntity. v1: minimal validation. */
  validate(): { success: boolean; error?: string } {
    if (!this.name || this.name.trim().length === 0) {
      return { success: false, error: 'ForgeRecipe.name must be non-empty' };
    }
    if (!this.recipeVersion || this.recipeVersion.trim().length === 0) {
      return { success: false, error: 'ForgeRecipe.recipeVersion must be non-empty (semver)' };
    }
    if (!this.source) {
      return { success: false, error: 'ForgeRecipe.source must be set (baseModel + architecture)' };
    }
    if (this.cycles < 1) {
      return { success: false, error: 'ForgeRecipe.cycles must be >= 1' };
    }
    return { success: true };
  }
}
