/**
 * Chat Message Transformation Utilities
 *
 * Provides explicit transformation functions for chat message entities,
 * particularly for handling media extraction and normalization.
 *
 * WHY MEDIA EXTRACTION IS NEEDED:
 * --------------------------------
 * When reading chat messages, media items are stored within the content.media array.
 * However, for API responses and UI consumption, we want to:
 * 1. Expose media at the top level for easier access
 * 2. Avoid duplicating large media data in the response payload
 * 3. Maintain a clean separation between message content and attachments
 *
 * This transformation extracts media to the result's top-level 'media' field
 * and removes it from the entity's content.media to prevent duplication.
 */

import type { ChatMessageEntity, MediaItem } from '../../../system/data/entities/ChatMessageEntity';
import type { BaseEntity } from '../../../system/data/entities/BaseEntity';

/**
 * Result of extracting media from a chat message
 */
export interface MediaExtractionResult<T extends BaseEntity> {
  /** The cleaned entity with media removed from content */
  entity: T;
  /** Extracted media items */
  media: MediaItem[];
}

/**
 * Extract media from a chat message entity to avoid duplication in API responses.
 *
 * Creates a new entity with media moved from content.media to a separate array,
 * preventing large media data from being duplicated in both locations.
 *
 * This function is designed to work with any BaseEntity but only performs
 * transformation if the entity has the ChatMessageEntity structure with media.
 *
 * @param entity - The entity to check and potentially transform
 * @returns Object containing the cleaned entity and extracted media array
 *
 * @example
 * const { entity: cleanedMessage, media } = extractMediaFromMessage(messageEntity);
 * return {
 *   ...result,
 *   data: cleanedMessage,
 *   media  // Media available at top level
 * };
 */
export function extractMediaFromMessage<T extends BaseEntity>(
  entity: T
): MediaExtractionResult<T> {
  // Type-safe check: verify this entity has the chat message structure
  // We check for the specific properties we need before casting
  if (!('content' in entity)) {
    // Not a chat message - return entity unchanged
    return {
      entity,
      media: []
    };
  }

  const content = (entity as { content?: unknown }).content;
  if (
    !content ||
    typeof content !== 'object' ||
    !('media' in content) ||
    !Array.isArray((content as { media?: unknown }).media)
  ) {
    // No media to extract - return entity unchanged
    return {
      entity,
      media: []
    };
  }

  // Now we can safely cast since we've verified the structure
  const messageData = entity as unknown as ChatMessageEntity;
  const media = messageData.content.media!;

  // Create cleaned entity without media duplication
  // IMPORTANT: We use Object.assign with Object.create to preserve the prototype chain.
  // This is critical for entity instances which may have methods or behaviors.
  // A simple spread operator would lose the prototype and break entity functionality.
  const cleanedEntity = Object.assign(
    Object.create(Object.getPrototypeOf(messageData)),
    messageData,
    {
      content: {
        ...messageData.content,
        media: []  // Clear media array to avoid duplication
      }
    }
  ) as T;

  return {
    entity: cleanedEntity,
    media: [...media]  // Create new array to prevent reference sharing
  };
}

/**
 * Check if an entity is a chat message that might contain media.
 *
 * Performs both collection name check and structural validation to ensure
 * the entity has the expected ChatMessageEntity properties.
 *
 * @param collection - The collection name
 * @param entity - The entity to check
 * @returns True if this is a chat message entity with valid structure
 */
export function isChatMessageEntity(
  collection: string,
  entity: BaseEntity
): entity is ChatMessageEntity {
  // First check collection name
  if (collection !== 'chat_messages') {
    return false;
  }

  // Then validate entity has the expected structure
  // At minimum, a ChatMessageEntity should have content property
  return (
    'content' in entity &&
    entity.content !== null &&
    typeof entity.content === 'object'
  );
}
