/**
 * TypeScript → Rust Type Generator
 *
 * Generates Rust structs from TypeScript entity decorators
 * Single source of truth: TypeScript entities define schema
 *
 * Usage:
 *   npx tsx generator/generate-rust-types.ts
 *
 * Output:
 *   workers/data-daemon/src/entities.rs
 */

import fs from 'fs';
import path from 'path';
import {
  getFieldMetadata,
  type FieldMetadata,
  type FieldType
} from '../system/data/decorators/FieldDecorators';
import {
  ENTITY_REGISTRY,
  initializeEntityRegistry,
  type EntityConstructor
} from '../daemons/data-daemon/server/EntityRegistry';

/**
 * Map TypeScript field types to Rust types
 */
function mapFieldTypeToRust(fieldType: FieldType, options?: FieldMetadata['options']): string {
  const nullable = options?.nullable ?? false;

  let rustType: string;

  switch (fieldType) {
    case 'primary':
      rustType = 'String'; // UUIDs are strings
      break;
    case 'foreign_key':
      rustType = 'String'; // Foreign keys are UUID strings
      break;
    case 'date':
      rustType = 'String'; // ISO date strings
      break;
    case 'enum':
      rustType = 'String'; // Enums stored as strings
      break;
    case 'text':
      rustType = 'String';
      break;
    case 'json':
      rustType = 'serde_json::Value'; // JSON blobs
      break;
    case 'number':
      rustType = 'i64'; // Use i64 for numbers (can hold both integers and version numbers)
      break;
    case 'boolean':
      rustType = 'bool';
      break;
    default:
      rustType = 'String'; // Default to String
  }

  // Wrap in Option<T> if nullable
  return nullable ? `Option<${rustType}>` : rustType;
}

/**
 * Rust reserved keywords that need to be escaped with r#
 */
const RUST_KEYWORDS = new Set([
  'as', 'async', 'await', 'break', 'const', 'continue', 'crate', 'dyn', 'else',
  'enum', 'extern', 'false', 'fn', 'for', 'if', 'impl', 'in', 'let', 'loop',
  'match', 'mod', 'move', 'mut', 'pub', 'ref', 'return', 'self', 'Self',
  'static', 'struct', 'super', 'trait', 'true', 'type', 'unsafe', 'use',
  'where', 'while', 'abstract', 'become', 'box', 'do', 'final', 'macro',
  'override', 'priv', 'typeof', 'unsized', 'virtual', 'yield'
]);

/**
 * Convert camelCase to snake_case for Rust field names
 * Escapes Rust keywords with r# prefix
 */
function toSnakeCase(str: string): string {
  const snakeCase = str.replace(/[A-Z]/g, letter => `_${letter.toLowerCase()}`);
  // Escape if either the original or snake_case version is a Rust keyword
  if (RUST_KEYWORDS.has(str) || RUST_KEYWORDS.has(snakeCase)) {
    return `r#${snakeCase}`;
  }
  return snakeCase;
}

/**
 * Convert collection name to PascalCase for Rust struct name
 * Examples: "chat_messages" → "ChatMessage", "users" → "User"
 */
function collectionToStructName(collectionName: string): string {
  // Split by underscore, capitalize each part, join
  return collectionName
    .split('_')
    .map(part => part.charAt(0).toUpperCase() + part.slice(1))
    .join('');
}

/**
 * Generate Rust struct for an entity
 */
function generateRustStruct(
  entityClass: EntityConstructor,
  collectionName: string
): string {
  // Use collection name to generate struct name (ensures uniqueness)
  // "chat_messages" → "ChatMessages", "users" → "Users"
  const structName = collectionToStructName(collectionName);
  const metadata = getFieldMetadata(entityClass);

  // Generate struct fields
  const fields: string[] = [];

  for (const [fieldName, fieldMetadata] of metadata) {
    const rustFieldName = toSnakeCase(fieldName);
    const rustType = mapFieldTypeToRust(fieldMetadata.fieldType, fieldMetadata.options);

    // Add serde attributes for field renaming (camelCase in JSON, snake_case in Rust)
    if (rustFieldName !== fieldName) {
      fields.push(`    #[serde(rename = "${fieldName}")]`);
    }

    // Add skip_serializing_if for Option types
    if (fieldMetadata.options?.nullable) {
      fields.push(`    #[serde(skip_serializing_if = "Option::is_none")]`);
    }

    fields.push(`    pub ${rustFieldName}: ${rustType},`);
  }

  // Generate struct with serde derives
  return `
/// ${structName} - Generated from ${entityClass.name}
/// Collection: ${collectionName}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ${structName} {
${fields.join('\n')}
}

impl ${structName} {
    /// Get the collection name for this entity type
    pub fn collection() -> &'static str {
        "${collectionName}"
    }
}
`.trim();
}

/**
 * Generate complete entities.rs file
 */
function generateEntitiesFile(): string {
  const structs: string[] = [];

  // Sort entity classes by collection name for stable output
  const sortedEntities = Array.from(ENTITY_REGISTRY.entries())
    .sort((a, b) => a[0].localeCompare(b[0]));

  for (const [collectionName, entityClass] of sortedEntities) {
    const rustStruct = generateRustStruct(entityClass, collectionName);
    structs.push(rustStruct);
  }

  // Generate file header
  const header = `//! Entity Types - Generated from TypeScript Decorators
//!
//! DO NOT EDIT THIS FILE MANUALLY
//! Generated by: generator/generate-rust-types.ts
//! Source of truth: TypeScript entity classes with field decorators
//!
//! To regenerate: npm run generate:rust-types

use serde::{Deserialize, Serialize};
use serde_json;
`;

  return header + '\n' + structs.join('\n\n') + '\n';
}

/**
 * Main generator function
 */
async function main() {
  console.log('🦀 Generating Rust types from TypeScript entities...\n');

  // Initialize entity registry (loads all entities and decorator metadata)
  initializeEntityRegistry();

  console.log(`📊 Found ${ENTITY_REGISTRY.size} entity types:\n`);

  // Show discovered entities
  const sortedCollections = Array.from(ENTITY_REGISTRY.keys()).sort();
  for (const collectionName of sortedCollections) {
    const entityClass = ENTITY_REGISTRY.get(collectionName)!;
    const fieldCount = getFieldMetadata(entityClass).size;
    console.log(`  ✅ ${collectionName.padEnd(35)} (${entityClass.name.padEnd(40)} - ${fieldCount} fields)`);
  }

  console.log('\n🔧 Generating Rust structs...\n');

  // Generate entities.rs content
  const content = generateEntitiesFile();

  // Write to output file
  const outputPath = path.join(
    process.cwd(),
    'workers/data-daemon/src/entities.rs'
  );

  fs.writeFileSync(outputPath, content, 'utf-8');

  console.log(`✅ Generated ${outputPath}`);
  console.log(`📦 ${ENTITY_REGISTRY.size} entity types`);
  console.log(`📏 ${content.split('\n').length} lines\n`);

  console.log('🎯 Next steps:');
  console.log('  1. Add to Rust worker: mod entities;');
  console.log('  2. Use entities::<EntityName> in operations');
  console.log('  3. Rebuild worker: cd workers/data-daemon && cargo build --release\n');

  console.log('✨ Done!\n');

  // Force exit to prevent Node.js hang
  process.exit(0);
}

main().catch(error => {
  console.error('❌ Error generating Rust types:', error);
  process.exit(1);
});
