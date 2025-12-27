/**
 * Recipe Entity - Composable command pipelines that define how humans and AIs collaborate
 *
 * Recipes are templates for conversation patterns, stored in database and loaded from JSON files
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

/**
 * Recipe Entity - Collaboration pattern template
 *
 * Contains command pipelines, RAG templates, and behavioral strategies
 */
export class RecipeEntity extends BaseEntity {
  static readonly collection = 'recipes';

  get collection(): string {
    return RecipeEntity.collection;
  }

  @TextField({ maxLength: TEXT_LENGTH.DEFAULT, index: true })
  uniqueId!: string;

  @TextField({ maxLength: TEXT_LENGTH.DEFAULT })
  name!: string;

  @TextField({ maxLength: TEXT_LENGTH.DEFAULT })
  displayName!: string;

  @TextField({ maxLength: TEXT_LENGTH.LONG })
  description!: string;

  @JsonField()
  pipeline!: RecipeStep[];

  @JsonField()
  ragTemplate!: RAGTemplate;

  @JsonField()
  strategy!: RecipeStrategy;

  @JsonField({ nullable: true })
  layout?: ActivityUILayout;

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

  // Required BaseEntity members
  validate(): { success: boolean; error?: string } {
    const errors: string[] = [];
    if (!this.uniqueId) errors.push('uniqueId is required');
    if (!this.name) errors.push('name is required');
    if (!this.displayName) errors.push('displayName is required');
    if (!this.pipeline || !Array.isArray(this.pipeline)) errors.push('pipeline is required');
    if (!this.ragTemplate) errors.push('ragTemplate is required');
    if (!this.strategy) errors.push('strategy is required');

    if (errors.length > 0) {
      return { success: false, error: errors.join(', ') };
    }
    return { success: true };
  }
}
