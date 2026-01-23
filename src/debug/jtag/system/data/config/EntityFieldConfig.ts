/**
 * Entity Field Configuration - Field Extraction Mapping
 *
 * Defines which fields from entities should be extracted to dedicated columns
 * vs stored as JSON blobs. This enables efficient querying while maintaining
 * backward compatibility with existing JSON storage.
 */

export type FieldExtractionType = 'text' | 'integer' | 'real' | 'boolean' | 'datetime' | 'json';

export interface ExtractedField {
  fieldName: string;
  sqliteType: FieldExtractionType;
  indexed?: boolean;
  nullable?: boolean;
  converter?: {
    toStorage: (value: any) => any;
    fromStorage: (value: any) => any;
  };
}

export interface EntityFieldMapping {
  collection: string;
  extractedFields: ExtractedField[];
  keepJsonBlob: boolean; // For backward compatibility during migration
}

/**
 * Field extraction configuration for existing entities
 */
export const ENTITY_FIELD_MAPPINGS: EntityFieldMapping[] = [
  {
    collection: 'chat_messages',
    keepJsonBlob: true,
    extractedFields: [
      {
        fieldName: 'messageId',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'senderId',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'roomId',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'timestamp',
        sqliteType: 'datetime',
        indexed: true,
        nullable: false,
        converter: {
          toStorage: (isoString: string) => new Date(isoString).getTime(),
          fromStorage: (timestamp: number) => new Date(timestamp).toISOString()
        }
      },
      {
        fieldName: 'messageType',
        sqliteType: 'text',
        indexed: true,
        nullable: true
      }
    ]
  },
  {
    collection: 'users',
    keepJsonBlob: true,
    extractedFields: [
      {
        fieldName: 'userId',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'type',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'displayName',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'lastActiveAt',
        sqliteType: 'datetime',
        indexed: true,
        nullable: true,
        converter: {
          toStorage: (isoString: string) => isoString ? new Date(isoString).getTime() : null,
          fromStorage: (timestamp: number) => timestamp ? new Date(timestamp).toISOString() : null
        }
      }
    ]
  },
  {
    collection: 'rooms',
    keepJsonBlob: true,
    extractedFields: [
      {
        fieldName: 'roomId',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'name',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'roomType',
        sqliteType: 'text',
        indexed: true,
        nullable: true
      },
      {
        fieldName: 'createdAt',
        sqliteType: 'datetime',
        indexed: true,
        nullable: false,
        converter: {
          toStorage: (isoString: string) => new Date(isoString).getTime(),
          fromStorage: (timestamp: number) => new Date(timestamp).toISOString()
        }
      }
    ]
  },
  /**
   * FeedbackEntity - Cross-AI Learning Patterns
   * Enables personas to share successful patterns and collectively improve
   */
  {
    collection: 'feedback_patterns',
    keepJsonBlob: true,
    extractedFields: [
      {
        fieldName: 'sourcePersonaId',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'type',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'domain',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'status',
        sqliteType: 'text',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'confidence',
        sqliteType: 'real',
        indexed: true,
        nullable: false
      },
      {
        fieldName: 'discoveredAt',
        sqliteType: 'datetime',
        indexed: true,
        nullable: false,
        converter: {
          toStorage: (isoString: string) => new Date(isoString).getTime(),
          fromStorage: (timestamp: number) => new Date(timestamp).toISOString()
        }
      },
      {
        fieldName: 'isPublic',
        sqliteType: 'boolean',
        indexed: true,
        nullable: false
      }
    ]
  }
];

/**
 * Get field mapping for a collection
 */
export function getEntityFieldMapping(collection: string): EntityFieldMapping | undefined {
  return ENTITY_FIELD_MAPPINGS.find(mapping => mapping.collection === collection);
}

/**
 * Check if a collection has field extraction configured
 */
export function hasFieldExtraction(collection: string): boolean {
  const mapping = getEntityFieldMapping(collection);
  return mapping ? mapping.extractedFields.length > 0 : false;
}