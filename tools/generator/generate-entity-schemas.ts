#!/usr/bin/env tsx
/**
 * generate-entity-schemas — Entity schema codegen (Phase 2 of ORM refactor)
 *
 * Walks ENTITY_REGISTRY, extracts decorator metadata for each registered
 * entity, emits protocol/typescript/entity_schemas.json for Rust to load
 * at module init.
 *
 * Source of truth: TS decorators (FieldDecorators.ts). Consumer: Rust's
 * modules/data.rs. Purpose: eliminate CollectionSchema from the IPC wire —
 * Rust already knows the entity shape after loading this file, so
 * ensure_schema only needs the collection name, not the schema structure.
 *
 * Design doc: docs/architecture/ORM-PHASE-2-DESIGN.md
 *
 * Run manually:   npx tsx ../tools/generator/generate-entity-schemas.ts (from src/)
 * Run from build: wired into `npm run build:ts` so Rust always has the
 *                 freshest entity shape on every rebuild.
 */

import * as fs from 'fs';
import * as path from 'path';
import { createHash } from 'crypto';

import {
  ENTITY_REGISTRY,
  initializeEntityRegistry,
} from '../../src/daemons/data-daemon/server/EntityRegistry';
import {
  getFieldMetadata,
  getCompositeIndexes,
  getArchiveConfig,
  type FieldMetadata,
  type CompositeIndexMetadata,
  type ArchiveConfig,
} from '../../src/system/data/decorators/FieldDecorators';

// ─── JSON shape (kept in sync with Rust's EntitySchemaJSON via ts-rs) ─────

interface EntitySchemaJSON {
  collection: string;
  entityClass: string;
  fields: FieldMetadata[];
  compositeIndexes: CompositeIndexMetadata[];
  archive: ArchiveConfig | null;
}

interface EntitySchemasFile {
  $schemaVersion: 1;
  $generatedAt: string;
  $sha256: string;
  entities: Record<string, EntitySchemaJSON>;
}

// ─── Canonicalization for stable hashing ──────────────────────────────────

/**
 * Produce a canonical representation of `value` suitable for hashing.
 * - Object keys are sorted
 * - Arrays preserve order (order is semantic)
 * - Primitives pass through
 *
 * Must match whatever Rust uses to verify the hash. Rust will deserialize
 * into the typed struct and re-serialize with sorted keys using the same
 * canonicalization rule.
 */
function canonicalize(value: unknown): unknown {
  if (value === null || typeof value !== 'object') return value;
  if (Array.isArray(value)) return value.map(canonicalize);
  const sorted: Record<string, unknown> = {};
  for (const key of Object.keys(value as Record<string, unknown>).sort()) {
    sorted[key] = canonicalize((value as Record<string, unknown>)[key]);
  }
  return sorted;
}

// ─── Main ─────────────────────────────────────────────────────────────────

async function main() {
  // Fire Stage 3 decorator initializers by instantiating each registered
  // entity class once. See EntityRegistry.ts:100 for the pattern.
  initializeEntityRegistry();

  if (ENTITY_REGISTRY.size === 0) {
    throw new Error(
      'ENTITY_REGISTRY is empty after initializeEntityRegistry() — decorator initializers did not fire. Verify every entity is imported in EntityRegistry.ts.'
    );
  }

  const entities: Record<string, EntitySchemaJSON> = {};
  for (const [collection, EntityClass] of ENTITY_REGISTRY.entries()) {
    const fieldMap = getFieldMetadata(EntityClass);
    const fields: FieldMetadata[] = Array.from(fieldMap.values());
    const compositeIndexes = getCompositeIndexes(EntityClass);
    const archive = getArchiveConfig(EntityClass);

    entities[collection] = {
      collection,
      entityClass: EntityClass.name,
      fields,
      compositeIndexes,
      archive,
    };
  }

  // Stable hash over canonical entities. `$generatedAt` and `$sha256` are
  // intentionally excluded from the hash — the first changes on every run,
  // the second IS the hash.
  const canonical = JSON.stringify(canonicalize({ entities }));
  const sha256 = createHash('sha256').update(canonical).digest('hex');

  const output: EntitySchemasFile = {
    $schemaVersion: 1,
    $generatedAt: new Date().toISOString(),
    $sha256: sha256,
    entities,
  };

  // protocol/typescript/entity_schemas.json is the tracked location — it is
  // what Rust include_str!'s (modules/entity_schemas.rs) and what the header
  // above documents. The old '../shared/generated' resolution was relative to
  // the pre-substrate src/scripts home and silently dumped output into
  // tools/shared/generated/ after the tools/ move, leaving the tracked file
  // stale (e.g. the deleted social subsystem lingered in it).
  const outPath = path.resolve(__dirname, '..', '..', 'protocol', 'typescript', 'entity_schemas.json');
  await fs.promises.mkdir(path.dirname(outPath), { recursive: true });

  // Idempotent write: if the ENTITIES section is identical to what's on
  // disk, skip rewriting so the file mtime stays stable and downstream
  // builds don't re-trigger on no-op codegen.
  try {
    const existing = JSON.parse(await fs.promises.readFile(outPath, 'utf8')) as EntitySchemasFile;
    if (existing.$sha256 === sha256) {
      console.log(`⏭️  entity_schemas.json unchanged (${Object.keys(entities).length} entities, sha=${sha256.substring(0, 12)})`);
      return;
    }
  } catch {
    // File doesn't exist or is invalid — proceed with write.
  }

  await fs.promises.writeFile(outPath, JSON.stringify(output, null, 2) + '\n');

  console.log(`✓ Generated ${path.relative(path.resolve(__dirname, '..', '..'), outPath)}`);
  console.log(`  Entities: ${Object.keys(entities).length}`);
  console.log(`  SHA-256:  ${sha256.substring(0, 16)}...`);
}

main()
  .then(() => {
    // Explicit exit: some entity imports leave open handles (loggers,
    // IPC sockets) that prevent Node from exiting on its own. Without
    // this, the script completes its work and then hangs in kevent
    // forever, blocking npm start. Verified 2026-04-20 via `sample`.
    process.exit(0);
  })
  .catch((err) => {
    console.error('❌ generate-entity-schemas failed:', err);
    process.exit(1);
  });
