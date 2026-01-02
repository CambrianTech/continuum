/**
 * PersonaRAGContextEntity - Stores persona conversation context for RAG
 *
 * Enables personas to maintain conversation context across sessions.
 * Each persona has one context record per room.
 */

import { BaseEntity } from './BaseEntity';
import { TextField, JsonField } from '../decorators/FieldDecorators';
import type { UUID } from '../../core/types/CrossPlatformUUID';

/**
 * Collection name for persona RAG contexts
 */
export const PERSONA_RAG_CONTEXTS_COLLECTION = 'persona_rag_contexts';

/**
 * PersonaRAGContextEntity - Persists working memory for personas
 */
export class PersonaRAGContextEntity extends BaseEntity {
  static readonly collection = PERSONA_RAG_CONTEXTS_COLLECTION;

  @TextField({ index: true })
  personaId!: UUID;

  @TextField({ index: true })
  roomId!: UUID;

  @JsonField()
  contextJson!: string;  // JSON stringified PersonaRAGContext

  constructor() {
    super();
  }

  get collection(): string {
    return PersonaRAGContextEntity.collection;
  }

  validate(): { success: boolean; error?: string } {
    if (!this.personaId) {
      return { success: false, error: 'personaId is required' };
    }
    if (!this.roomId) {
      return { success: false, error: 'roomId is required' };
    }
    return { success: true };
  }
}
