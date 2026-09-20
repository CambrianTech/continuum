/**
 * Field Mapping Constants - Centralized type-safe field definitions
 *
 * "types defined at the source and imported by all constants"
 *
 * This provides a centralized mapping between domain field names and database-specific
 * column names, allowing storage adapters to be implementation-agnostic while maintaining
 * type safety.
 */

import type { FieldName } from './DataTypes';

// Domain field names (generic, used in business logic)
export const DOMAIN_FIELDS = {
  // Timestamp fields (ISO strings in domain, potentially different in storage)
  CREATED_AT: 'createdAt' as const,
  UPDATED_AT: 'updatedAt' as const,
  TIMESTAMP: 'timestamp' as const,
  EDITED_AT: 'editedAt' as const,
  DELETED_AT: 'deletedAt' as const,

  // Identifier fields
  ID: 'id' as const,
  USER_ID: 'userId' as const,
  MESSAGE_ID: 'messageId' as const,
  ROOM_ID: 'roomId' as const,
  SENDER_ID: 'senderId' as const,

  // Content fields
  CONTENT: 'content' as const,
  DISPLAY_NAME: 'displayName' as const,

  // Metadata fields
  VERSION: 'version' as const,
  CATEGORY: 'category' as const,
  CITIZEN_TYPE: 'citizenType' as const
} as const;

// SQLite-specific column names (snake_case convention)
export const SQLITE_COLUMNS = {
  // Timestamp columns (SQLite uses snake_case)
  CREATED_AT: 'created_at' as const,
  UPDATED_AT: 'updated_at' as const,
  TIMESTAMP: 'created_at' as const, // Map domain 'timestamp' to SQLite 'created_at'
  EDITED_AT: 'edited_at' as const,
  DELETED_AT: 'deleted_at' as const,

  // Identifier columns
  ID: 'id' as const,
  USER_ID: 'user_id' as const,
  MESSAGE_ID: 'message_id' as const,
  ROOM_ID: 'room_id' as const,
  SENDER_ID: 'sender_id' as const,

  // Content columns (stored in JSON data column)
  CONTENT: `JSON_EXTRACT(data, '$.content')` as const,
  DISPLAY_NAME: `JSON_EXTRACT(data, '$.displayName')` as const,

  // Metadata columns
  VERSION: 'version' as const,
  CATEGORY: `JSON_EXTRACT(data, '$.category')` as const,
  CITIZEN_TYPE: `JSON_EXTRACT(data, '$.citizenType')` as const
} as const;

// Type-safe mapping function from domain fields to SQLite columns
export const mapFieldToSqlColumn = (domainField: string): string => {
  // Create reverse lookup from DOMAIN_FIELDS values to SQLITE_COLUMNS
  const fieldMapping: Record<string, string> = {
    [DOMAIN_FIELDS.CREATED_AT]: SQLITE_COLUMNS.CREATED_AT,
    [DOMAIN_FIELDS.UPDATED_AT]: SQLITE_COLUMNS.UPDATED_AT,
    [DOMAIN_FIELDS.TIMESTAMP]: SQLITE_COLUMNS.TIMESTAMP,
    [DOMAIN_FIELDS.EDITED_AT]: SQLITE_COLUMNS.EDITED_AT,
    [DOMAIN_FIELDS.DELETED_AT]: SQLITE_COLUMNS.DELETED_AT,

    [DOMAIN_FIELDS.ID]: SQLITE_COLUMNS.ID,
    [DOMAIN_FIELDS.USER_ID]: SQLITE_COLUMNS.USER_ID,
    [DOMAIN_FIELDS.MESSAGE_ID]: SQLITE_COLUMNS.MESSAGE_ID,
    [DOMAIN_FIELDS.ROOM_ID]: SQLITE_COLUMNS.ROOM_ID,
    [DOMAIN_FIELDS.SENDER_ID]: SQLITE_COLUMNS.SENDER_ID,

    [DOMAIN_FIELDS.CONTENT]: SQLITE_COLUMNS.CONTENT,
    [DOMAIN_FIELDS.DISPLAY_NAME]: SQLITE_COLUMNS.DISPLAY_NAME,

    [DOMAIN_FIELDS.VERSION]: SQLITE_COLUMNS.VERSION,
    [DOMAIN_FIELDS.CATEGORY]: SQLITE_COLUMNS.CATEGORY,
    [DOMAIN_FIELDS.CITIZEN_TYPE]: SQLITE_COLUMNS.CITIZEN_TYPE
  };

  // Return mapped column or default to JSON extraction for unmapped fields
  return fieldMapping[domainField] || `JSON_EXTRACT(data, '$.${domainField}')`;
};

// Type-safe field name constructor
export const DomainField = (name: string): FieldName => name as FieldName;

// Collection constants (for type safety)
export const COLLECTIONS = {
  CHAT_MESSAGES: 'ChatMessage' as const,
  USERS: 'User' as const,
  ROOMS: 'Room' as const,
  SESSIONS: 'sessions' as const
} as const;

// Export type definitions for external use
export type DomainFieldName = typeof DOMAIN_FIELDS[keyof typeof DOMAIN_FIELDS];
export type SqliteColumnName = typeof SQLITE_COLUMNS[keyof typeof SQLITE_COLUMNS];
export type CollectionName = typeof COLLECTIONS[keyof typeof COLLECTIONS];