/**
 * Recipe Entity - Composable command pipelines that define how humans and AIs collaborate
 *
 * Recipes are the ORM source of truth for room/activity configuration.
 * JSON files on disk are seed data. At runtime, recipes live in the database.
 *
 * Every recipe defines:
 *   - WHAT it looks like (layout: widgets + positions)
 *   - HOW it behaves (pipeline, strategy, ragTemplate)
 *   - WHO participates (team)
 *   - WHERE it lives in URLs (view)
 *   - WHAT entity it backs (entityType)
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import { BaseEntity } from './BaseEntity';
import { TextField, JsonField, DateField, TEXT_LENGTH } from '../decorators/FieldDecorators';
import type {
  RecipeStep,
  RAGTemplate,
  RecipeStrategy,
  ActivityUILayout
} from '../../../system/recipes/shared/RecipeTypes';

/** Entity types a recipe can back */
export type RecipeEntityType = 'room' | 'user' | 'activity' | null;

/** Widget placement in the new layout format */
export interface RecipeLayoutWidget {
  widget: string;
  position: 'left' | 'center' | 'right';
  order: number;
  config?: Record<string, unknown>;
  title?: string;
  icon?: string;
  flex?: number;
}

/** New-format layout with positioned widgets */
export interface RecipeLayoutConfig {
  widgets: RecipeLayoutWidget[];
  panels?: {
    left?: { visible: boolean };
    center?: { visible: boolean };
    right?: { visible: boolean };
  };
}

/** Layout accepts both old (ActivityUILayout) and new (RecipeLayoutConfig) formats */
export type RecipeLayout = ActivityUILayout | RecipeLayoutConfig;

/**
 * Recipe Entity - Collaboration pattern template
 *
 * Contains command pipelines, RAG templates, behavioral strategies,
 * and UI layout composition. This is the ORM entity — the single source
 * of truth at runtime.
 */
export class RecipeEntity extends BaseEntity {
  static readonly collection = 'recipes';

  get collection(): string {
    return RecipeEntity.collection;
  }

  // === Identity ===

  @TextField({ maxLength: TEXT_LENGTH.DEFAULT, index: true })
  uniqueId!: string;

  @TextField({ maxLength: TEXT_LENGTH.DEFAULT })
  name!: string;

  @TextField({ maxLength: TEXT_LENGTH.DEFAULT })
  displayName!: string;

  @TextField({ maxLength: TEXT_LENGTH.LONG })
  description!: string;

  // === URL & Entity Binding ===

  /** URL prefix — verb/noun pattern: /chat, /live, /factory */
  @TextField({ maxLength: TEXT_LENGTH.SHORT })
  view!: string;

  /** What kind of entity this recipe backs — room, user, activity, or null (singleton) */
  @JsonField({ nullable: true })
  entityType!: RecipeEntityType;

  // === Layout ===

  /** UI composition — widgets + positions. Accepts old or new format. */
  @JsonField({ nullable: true })
  layout?: RecipeLayout;

  /** Paths that cannot be modified by users (e.g., "layout.main") */
  @JsonField({ nullable: true })
  locked?: string[];

  // === Behavior ===

  @JsonField()
  pipeline!: RecipeStep[];

  @JsonField()
  ragTemplate!: RAGTemplate;

  @JsonField()
  strategy!: RecipeStrategy;

  // === Team & Modes ===

  /** AI personas assigned to this recipe's room. null = everyone joins. */
  @JsonField({ nullable: true })
  team?: string[] | null;

  /** Available view modes (e.g., ["chat", "live", "forge"]) */
  @JsonField({ nullable: true })
  modes?: string[];

  // === Metadata ===

  @JsonField()
  tags!: string[];

  @TextField({ maxLength: TEXT_LENGTH.SHORT })
  createdBy!: UUID;

  @JsonField({ nullable: true })
  parentRecipeId?: UUID;

  @JsonField()
  usageCount!: number;

  @DateField()
  lastUsedAt!: Date;

  @JsonField()
  isPublic!: boolean;

  // Index signature for compatibility
  [key: string]: unknown;

  // === Validation ===

  validate(): { success: boolean; error?: string } {
    const errors: string[] = [];
    if (!this.uniqueId) errors.push('uniqueId is required');
    if (!this.name) errors.push('name is required');
    if (!this.displayName) errors.push('displayName is required');
    if (!this.view) errors.push('view is required');
    if (!this.pipeline || !Array.isArray(this.pipeline)) errors.push('pipeline is required');
    if (!this.ragTemplate) errors.push('ragTemplate is required');
    if (!this.strategy) errors.push('strategy is required');

    if (errors.length > 0) {
      return { success: false, error: errors.join(', ') };
    }
    return { success: true };
  }
}
