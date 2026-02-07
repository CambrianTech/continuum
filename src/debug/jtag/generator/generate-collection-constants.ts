/**
 * Collection Constants Generator
 *
 * Automatically generates COLLECTIONS constant from entity definitions.
 * Entities are the SINGLE SOURCE OF TRUTH for collection names.
 *
 * How it works:
 * 1. Scan all entity files in system/data/entities/
 * 2. Extract `static readonly collection = '...'` from each
 * 3. Generate COLLECTIONS constant with type-safe keys
 * 4. Generate CollectionName type for ORM method signatures
 *
 * **Integration:**
 * - Runs automatically via prebuild script
 * - Import { COLLECTIONS, CollectionName } from '@shared/generated-collection-constants'
 * - ORM methods use CollectionName type, not string
 * - Never hardcode collection strings anywhere
 *
 * **Why this matters:**
 * - Entities define their collection (single source of truth)
 * - Type-safe collection usage throughout codebase
 * - Impossible to use invalid collection names (compile error)
 * - Add entity, run build, collection constant appears
 */

import { readFileSync, writeFileSync, existsSync } from 'fs';
import { join, basename } from 'path';
import * as glob from 'glob';

interface CollectionInfo {
  entityName: string;       // e.g., 'UserEntity'
  collectionName: string;   // e.g., 'users'
  constantKey: string;      // e.g., 'USERS'
  filePath: string;
}

class CollectionConstantsGenerator {
  private rootPath: string;
  private collections: CollectionInfo[] = [];

  constructor(rootPath: string) {
    this.rootPath = rootPath;
  }

  /**
   * Main entry point - discover all entities and generate constants
   */
  generate(): void {
    console.log('🔍 Scanning entities for collection definitions...');

    // Find all *Entity.ts files
    const entityPaths = [
      join(this.rootPath, 'system/data/entities/*Entity.ts'),
      join(this.rootPath, 'system/genome/entities/*Entity.ts'),
      join(this.rootPath, 'system/social/shared/*Entity.ts'),
      join(this.rootPath, 'daemons/data-daemon/shared/entities/*Entity.ts'),
    ];

    const allFiles: string[] = [];
    for (const pattern of entityPaths) {
      const files = glob.sync(pattern);
      allFiles.push(...files);
    }

    console.log(`📄 Found ${allFiles.length} entity files`);

    for (const filePath of allFiles) {
      try {
        const info = this.extractCollectionInfo(filePath);
        if (info) {
          this.collections.push(info);
          console.log(`  ✅ ${info.entityName}: '${info.collectionName}' → ${info.constantKey}`);
        }
      } catch (error) {
        console.error(`  ❌ Failed to extract from ${filePath}:`, error);
      }
    }

    // Sort by constant key for consistent output
    this.collections.sort((a, b) => a.constantKey.localeCompare(b.constantKey));

    console.log(`\n✅ Extracted ${this.collections.length} collections`);

    this.writeConstants();
    this.validateNoOrphans();
  }

  /**
   * Extract collection info from entity file
   */
  private extractCollectionInfo(filePath: string): CollectionInfo | null {
    const content = readFileSync(filePath, 'utf-8');
    const entityName = basename(filePath, '.ts');

    // Match: static readonly collection = 'collection_name';
    // or: static readonly collection = COLLECTIONS.CONSTANT;
    const directMatch = content.match(/static\s+readonly\s+collection\s*=\s*['"]([^'"]+)['"]/);

    if (directMatch) {
      const collectionName = directMatch[1];
      const constantKey = this.toConstantKey(collectionName);
      return { entityName, collectionName, constantKey, filePath };
    }

    // Handle COLLECTIONS.X references - extract the referenced constant
    const refMatch = content.match(/static\s+readonly\s+collection\s*=\s*COLLECTIONS\.(\w+)/);
    if (refMatch) {
      // This entity already uses COLLECTIONS - we need to find the actual value
      // For now, derive from the constant name
      const constantKey = refMatch[1];
      const collectionName = this.fromConstantKey(constantKey);
      return { entityName, collectionName, constantKey, filePath };
    }

    // No collection defined
    console.log(`  ⚠️  ${entityName}: No collection property found`);
    return null;
  }

  /**
   * Convert collection name to constant key
   * 'chat_messages' → 'CHAT_MESSAGES'
   * 'users' → 'USERS'
   */
  private toConstantKey(collectionName: string): string {
    return collectionName.toUpperCase().replace(/-/g, '_');
  }

  /**
   * Convert constant key back to collection name (for COLLECTIONS.X references)
   * 'CHAT_MESSAGES' → 'chat_messages'
   */
  private fromConstantKey(constantKey: string): string {
    return constantKey.toLowerCase();
  }

  /**
   * Write the generated constants file
   */
  private writeConstants(): void {
    const outputPath = join(this.rootPath, 'shared/generated-collection-constants.ts');

    const lines: string[] = [
      '/**',
      ' * Generated Collection Constants',
      ' *',
      ' * ⚠️  AUTO-GENERATED - DO NOT EDIT MANUALLY',
      ' * Source of truth: Entity files with `static readonly collection`',
      ' * Generator: generator/generate-collection-constants.ts',
      ' *',
      ' * Run: npx tsx generator/generate-collection-constants.ts',
      ' */',
      '',
      '/**',
      ' * Collection name constants - use these instead of hardcoded strings',
      ' * TypeScript will catch any typos at compile time',
      ' */',
      'export const COLLECTIONS = {',
    ];

    // Add each collection as a constant
    for (const info of this.collections) {
      lines.push(`  /** From ${info.entityName} */`);
      lines.push(`  ${info.constantKey}: '${info.collectionName}' as const,`);
    }

    lines.push('} as const;');
    lines.push('');
    lines.push('/**');
    lines.push(' * Type-safe collection name - use this in ORM method signatures');
    lines.push(' * Prevents passing arbitrary strings as collection names');
    lines.push(' */');
    lines.push('export type CollectionName = typeof COLLECTIONS[keyof typeof COLLECTIONS];');
    lines.push('');
    lines.push('/**');
    lines.push(' * Collection constant keys - for programmatic access');
    lines.push(' */');
    lines.push('export type CollectionKey = keyof typeof COLLECTIONS;');
    lines.push('');
    lines.push('/**');
    lines.push(' * Validate a string is a valid collection name (runtime check)');
    lines.push(' */');
    lines.push('export function isValidCollection(name: string): name is CollectionName {');
    lines.push('  return Object.values(COLLECTIONS).includes(name as CollectionName);');
    lines.push('}');
    lines.push('');
    lines.push('/**');
    lines.push(' * Get all collection names as array');
    lines.push(' */');
    lines.push('export function getAllCollections(): CollectionName[] {');
    lines.push('  return Object.values(COLLECTIONS);');
    lines.push('}');
    lines.push('');

    writeFileSync(outputPath, lines.join('\n'));
    console.log(`\n📝 Written to: ${outputPath}`);
  }

  /**
   * Check for collections in ORMConfig that don't have entities
   */
  private validateNoOrphans(): void {
    const ormConfigPath = join(this.rootPath, 'daemons/data-daemon/shared/ORMConfig.ts');
    if (!existsSync(ormConfigPath)) return;

    const content = readFileSync(ormConfigPath, 'utf-8');
    const validCollections = new Set(this.collections.map(c => c.collectionName));

    // Find all hardcoded collection strings in ORMConfig
    const hardcodedMatches = content.matchAll(/'([a-z_]+)':\s*\{/g);
    const orphans: string[] = [];

    for (const match of hardcodedMatches) {
      const collectionName = match[1];
      // Skip non-collection keys like 'rust', 'typescript', 'shadow'
      if (['rust', 'typescript', 'shadow', 'read', 'write', 'both'].includes(collectionName)) continue;

      if (!validCollections.has(collectionName)) {
        orphans.push(collectionName);
      }
    }

    if (orphans.length > 0) {
      console.log('\n⚠️  ORPHAN COLLECTIONS in ORMConfig (no entity found):');
      for (const orphan of orphans) {
        console.log(`  ❌ '${orphan}' - has no corresponding entity`);
      }
      console.log('\nFix: Remove these from ORMConfig or create entities for them.');
    }
  }
}

// ============================================================================
// MAIN
// ============================================================================

const rootPath = join(__dirname, '..');
const generator = new CollectionConstantsGenerator(rootPath);
generator.generate();
