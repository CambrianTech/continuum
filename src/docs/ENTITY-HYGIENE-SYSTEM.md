# Entity Hygiene System Design

**Purpose**: Prevent and cleanup entity-related issues caused by direct schema creation or incomplete operations.

## Problem

AIs are creating database schemas without backing TypeScript entity classes, causing:
- "Entity not registered" errors
- Schema/code mismatches
- Broken CRUD operations
- Database corruption

## Solution: Prevention + Cleanup

### 1. Prevention: Schema Creation Guard (ALREADY IMPLEMENTED ✅)

**Current Protection** (SqliteSchemaManager.ts:175-253):
- Schema creation ONLY happens through `ensureSchema()`
- `ensureSchema()` checks EntityRegistry for entity class
- If entity NOT registered OR missing field decorators → ERROR
- Error message instructs to create entity class properly

**This means**:
- ❌ AIs CANNOT create orphaned schemas (protection exists)
- ✅ Schemas ONLY created for registered entities with decorators
- ✅ Error messages guide to proper entity creation

**The Real Risk**:
AIs might try to manually edit `EntityRegistry.ts` to register entities without creating proper entity class files. This bypasses protection!

**Solution**:
- EntityRegistry.ts should be READ-ONLY for AIs
- Entity registration ONLY through generator: `./jtag generate entity-spec.json`
- Generator creates: entity class + decorators + registry entry + validation

**data/schema Command** (DataSchemaServerCommand.ts):
- This is READ-ONLY introspection (not creation)
- Returns schema info for EXISTING registered entities
- Does NOT create schemas or tables
- No changes needed here

### 2. Cleanup: Entity Hygiene Command

**Command**: `./jtag generate/entity-cleanup`

**Modes**:
- `--scan`: Report all issues (no changes)
- `--dry-run`: Show what would be fixed
- `--fix`: Auto-fix all safe issues
- `--interactive`: Choose fixes per issue

**Issue Types**:

#### A. Orphaned Schema (schema exists, no entity class)
```
genome_adapters schema exists
❌ No GenomeAdapterEntity.ts found

Options:
  1. Generate entity class from schema
  2. Delete schema (if no data)
  3. Skip (manual review needed)

Danger: SAFE (generates code)
```

#### B. Orphaned Entity Class (entity class exists, no schema)
```
PersonaTaskEntity.ts exists
❌ No persona_tasks schema in database

Options:
  1. Create schema from entity class
  2. Delete entity class
  3. Skip (manual review needed)

Danger: SAFE (creates schema)
```

#### C. Incomplete Removal (partial deletion)
```
TrainingDatasetEntity.ts deleted
✅ training_datasets schema exists
✅ Database has 150 records

Options:
  1. Regenerate entity class from schema
  2. Delete schema + data (DANGEROUS)
  3. Skip (manual review needed)

Danger: DATA-LOSS if option 2
```

#### D. Unregistered Entity (entity + schema exist, not in registry)
```
GenomeAdapterEntity.ts exists
genome_adapters schema exists
❌ Not in EntityRegistry

Options:
  1. Add to EntityRegistry
  2. Skip (manual review needed)

Danger: SAFE (registry update)
```

#### E. Collection Name Mismatch
```
GenomeAdapterEntity.ts:
  static readonly collection = 'genome_adapter' // Wrong!
Schema name: 'genome_adapters'

Options:
  1. Fix entity class collection name
  2. Rename schema
  3. Skip (manual review needed)

Danger: BREAKING (rename operations)
```

### 3. Entity Generator

**Command**: `./jtag generate entity-spec.json`

**Generates**:
- Entity class with decorators
- validate() method with rules
- Type definitions
- Schema (via data/schema with entity class)
- Registry entry

**Spec Format**:
```json
{
  "name": "GenomeAdapter",
  "collectionName": "genome_adapters",
  "description": "LoRA adapter for genome paging system",
  "fields": {
    "adapterId": {
      "type": "UUID",
      "optional": false,
      "description": "Unique adapter ID"
    },
    "domain": {
      "type": "string",
      "optional": false,
      "description": "Skill domain",
      "validation": {
        "enum": ["code", "chat", "reasoning", "creative"]
      }
    },
    "loadedAt": {
      "type": "Date",
      "optional": true,
      "description": "When adapter was loaded into memory"
    }
  }
}
```

### 4. Integration with Generate Command

**Add entity subcommand**:
```bash
./jtag generate entity-spec.json          # Generate entity
./jtag generate/entity-cleanup --scan      # Scan for issues
./jtag generate/audit --type=entity        # Validate all entities
```

## Implementation Plan

### Phase 1: Schema Guard (COMPLETE ✅)
**Status**: Protection already exists in SqliteSchemaManager.ts
- ✅ ensureSchema() checks EntityRegistry before creating tables
- ✅ Returns clear error if entity not registered or missing decorators
- ✅ Error messages guide AIs to create proper entity classes

**Remaining Work**: Document that EntityRegistry.ts should be READ-ONLY for AIs

### Phase 2: EntityGenerator (NEXT - HIGH PRIORITY)
- Implement ModuleGenerator extension
- Create entity templates
- Generate decorators, validation, types
- Test with genome_adapters spec

### Phase 3: Cleanup Command
- Scan filesystem for entity classes
- Scan database for schemas
- Compare with EntityRegistry
- Generate issue reports
- Implement fix strategies

### Phase 4: Audit Integration
- Add entity checks to generate/audit
- Validate decorator usage
- Check validate() methods
- Verify registry entries

## Success Criteria

✅ AIs can't create orphaned schemas (prevention)
✅ Existing orphans are detected (cleanup scan)
✅ Safe fixes applied automatically (cleanup --fix)
✅ Dangerous fixes require confirmation (interactive mode)
✅ Entity generation produces valid, complete entities
✅ All entities pass audit checks

## Benefits

- **Data integrity**: No more orphaned schemas
- **Type safety**: Entity classes match schemas
- **Developer experience**: Clear error messages
- **System health**: Regular hygiene maintenance
- **AI guardrails**: Prevent common mistakes
