/**
 * EntityGeneratorType — IGeneratorType implementation for entities
 *
 * Provides audit, fix, reverse-engineer, and help for entity classes.
 * Entities are single-file modules (system/data/entities/{Name}Entity.ts).
 */

import * as fs from 'fs';
import * as path from 'path';
import type { EntitySpec, EntityField } from '../EntityTypes';
import type {
  IGeneratorType,
  GeneratorAuditSummary,
  GeneratorAuditEntry,
  FixSummary,
  FixResult,
  GeneratorHelp
} from '../GeneratorSDK';
import type { GenerateOptions } from '../ModuleGenerator';
import { EntityGenerator } from '../EntityGenerator';

export class EntityGeneratorType implements IGeneratorType<EntitySpec> {
  readonly typeName = 'entity';
  readonly description = 'Entity classes with decorators, validation, and ORM integration';

  private readonly rootPath: string;
  private readonly generator: EntityGenerator;
  private readonly entitiesDir: string;
  private readonly specsDir: string;

  constructor(rootPath: string) {
    this.rootPath = rootPath;
    this.generator = new EntityGenerator(rootPath);
    this.entitiesDir = path.join(rootPath, 'system', 'data', 'entities');
    this.specsDir = path.join(rootPath, 'generator', 'specs', 'entities');
  }

  // ── Generate ────────────────────────────────────────────────────

  generate(spec: EntitySpec, outputDir?: string, options?: GenerateOptions): void {
    this.generator.generate(spec, outputDir, options);
  }

  generateFromFile(specFilePath: string, outputDir?: string, options?: GenerateOptions): void {
    const specJson = fs.readFileSync(specFilePath, 'utf-8');
    const spec: EntitySpec = JSON.parse(specJson);
    this.generator.generate(spec, outputDir, options);
  }

  // ── Audit ───────────────────────────────────────────────────────

  audit(): GeneratorAuditSummary {
    const entityFiles = this.discoverEntities();
    const specMap = this.loadSpecMap();
    const entries: GeneratorAuditEntry[] = [];

    const checkSummary: Record<string, { passing: number; failing: number }> = {
      'has-spec': { passing: 0, failing: 0 },
      'extends-base-entity': { passing: 0, failing: 0 },
      'has-collection': { passing: 0, failing: 0 },
      'has-validate': { passing: 0, failing: 0 },
      'has-decorators': { passing: 0, failing: 0 },
      'no-any-casts': { passing: 0, failing: 0 },
    };

    for (const { name, filePath } of entityFiles) {
      const content = fs.readFileSync(filePath, 'utf-8');
      const specName = name.replace(/Entity$/, '');
      const hasSpec = specName in specMap;

      const checks: Record<string, boolean> = {
        'has-spec': hasSpec,
        'extends-base-entity': /extends\s+BaseEntity/.test(content),
        'has-collection': /static\s+readonly\s+collection\s*=/.test(content),
        'has-validate': /validate\s*\(\s*\)/.test(content),
        'has-decorators': /@(TextField|NumberField|BooleanField|DateField|JsonField)/.test(content),
        'no-any-casts': !/(:\s*any\b|as\s+any\b)/.test(content),
      };

      const issues: string[] = [];
      const fixableIssues: string[] = [];

      if (!checks['extends-base-entity']) issues.push('Does not extend BaseEntity');
      if (!checks['has-collection']) issues.push('Missing static collection property');
      if (!checks['has-validate']) {
        issues.push('Missing validate() method');
        if (hasSpec) fixableIssues.push('Missing validate() (can regenerate from spec)');
      }
      if (!checks['has-decorators']) {
        issues.push('Missing field decorators');
        if (hasSpec) fixableIssues.push('Missing decorators (can regenerate from spec)');
      }
      if (!checks['no-any-casts']) issues.push('Contains any casts');
      if (!hasSpec) issues.push('No matching entity spec');

      for (const [checkName, passing] of Object.entries(checks)) {
        if (passing) checkSummary[checkName].passing++;
        else checkSummary[checkName].failing++;
      }

      entries.push({
        name,
        path: filePath,
        hasSpec,
        specPath: hasSpec ? specMap[specName] : undefined,
        checks,
        issues,
        fixableIssues,
      });
    }

    // Find orphaned specs
    const entityNames = new Set(entityFiles.map(e => e.name.replace(/Entity$/, '')));
    const orphanedSpecs = Object.keys(specMap).filter(name => !entityNames.has(name));

    return {
      type: this.typeName,
      entries,
      total: entityFiles.length,
      withSpecs: entries.filter(e => e.hasSpec).length,
      totalIssues: entries.reduce((sum, e) => sum + e.issues.length, 0),
      totalFixable: entries.reduce((sum, e) => sum + e.fixableIssues.length, 0),
      orphanedSpecs,
      checkSummary,
    };
  }

  auditOne(modulePath: string): GeneratorAuditEntry {
    const summary = this.audit();
    const entry = summary.entries.find(e =>
      e.path === modulePath || e.name === modulePath
    );
    if (!entry) {
      return {
        name: modulePath,
        path: modulePath,
        hasSpec: false,
        checks: {},
        issues: [`Entity not found: ${modulePath}`],
        fixableIssues: [],
      };
    }
    return entry;
  }

  // ── Fix ─────────────────────────────────────────────────────────

  fixAll(): FixSummary {
    const audit = this.audit();
    const results: FixResult[] = [];

    for (const entry of audit.entries) {
      if (entry.fixableIssues.length === 0) continue;
      const result = this.fixOne(entry.path);
      if (result.issuesFixed.length > 0) {
        results.push(result);
      }
    }

    return {
      type: this.typeName,
      results,
      totalFixed: results.reduce((sum, r) => sum + r.issuesFixed.length, 0),
      totalRemaining: results.reduce((sum, r) => sum + r.issuesRemaining.length, 0),
    };
  }

  fixOne(modulePath: string): FixResult {
    const entry = this.auditOne(modulePath);

    // Entity fix: regenerate the entire file from spec (entities are single-file)
    if (entry.hasSpec && entry.specPath) {
      try {
        const spec: EntitySpec = JSON.parse(fs.readFileSync(entry.specPath, 'utf-8'));
        this.generator.generate(spec, undefined, { force: true });
        return {
          name: entry.name,
          filesModified: [entry.path],
          filesCreated: [],
          issuesFixed: entry.fixableIssues,
          issuesRemaining: entry.issues.filter(i =>
            !entry.fixableIssues.some(f => i.includes(f.split(' ')[1]))
          ),
        };
      } catch (err) {
        return {
          name: entry.name,
          filesModified: [],
          filesCreated: [],
          issuesFixed: [],
          issuesRemaining: [`Failed to regenerate: ${err instanceof Error ? err.message : String(err)}`],
        };
      }
    }

    return {
      name: entry.name,
      filesModified: [],
      filesCreated: [],
      issuesFixed: [],
      issuesRemaining: entry.issues,
    };
  }

  // ── Reverse Engineer ────────────────────────────────────────────

  reverseEngineer(modulePath: string): EntitySpec | null {
    const filePath = modulePath.endsWith('.ts') ? modulePath :
      path.join(this.entitiesDir, `${modulePath}Entity.ts`);

    if (!fs.existsSync(filePath)) return null;
    const content = fs.readFileSync(filePath, 'utf-8');

    // Extract class name
    const classMatch = content.match(/export\s+class\s+(\w+)Entity/);
    if (!classMatch) return null;
    const name = classMatch[1];

    // Extract collection name
    const collectionMatch = content.match(/static\s+readonly\s+collection\s*=\s*'([^']+)'/);
    const collectionName = collectionMatch?.[1] || name.toLowerCase() + 's';

    // Extract description
    const descMatch = content.match(/\*\s+(\w+Entity)\s*-\s*(.+)/);
    const description = descMatch?.[2]?.trim() || 'TODO: Add description';

    // Extract fields from decorators + declarations
    const fields: Record<string, EntityField> = {};
    const fieldRegex = /@(\w+Field)\(([^)]*)\)\s*\n\s*(\w+)(\?)?\s*:\s*([^;]+);/g;
    let fieldMatch: RegExpExecArray | null;

    while ((fieldMatch = fieldRegex.exec(content)) !== null) {
      const [, decorator, , fieldName, optional, tsType] = fieldMatch;
      fields[fieldName] = {
        type: this.decoratorToFieldType(decorator),
        optional: !!optional,
        description: fieldName,
      };
    }

    return { name, collectionName, description, fields };
  }

  // ── Help & Templates ────────────────────────────────────────────

  help(): GeneratorHelp {
    const full = `
ENTITY GENERATOR
================

Generates entity classes with decorators, validation, and ORM integration.

SPEC FORMAT (EntitySpec):
  {
    "name": "UserProfile",                    // PascalCase, no 'Entity' suffix
    "collectionName": "user_profiles",        // snake_case table/collection name
    "description": "User profile data",
    "fields": {
      "displayName": {
        "type": "string",
        "description": "User display name",
        "validation": { "minLength": 1, "maxLength": 100 }
      },
      "age": {
        "type": "number",
        "optional": true,
        "description": "User age",
        "validation": { "min": 0, "max": 200 }
      }
    },
    "indexes": [
      { "fields": ["displayName"], "unique": true }
    ]
  }

FIELD TYPES:
  string    → @TextField    → string
  number    → @NumberField  → number
  boolean   → @BooleanField → boolean
  UUID      → @TextField    → UUID
  Date      → @DateField    → Date
  JSON      → @JsonField    → Record<string, unknown>
  array     → @JsonField    → T[]

VALIDATION RULES:
  String:  minLength, maxLength, pattern (regex)
  Number:  min, max
  Any:     enum (allowed values), message (custom error)

GENERATED OUTPUT:
  system/data/entities/{Name}Entity.ts
    - Class extending BaseEntity
    - Field decorators
    - Constructor with defaults
    - validate() method
    - Static collection property

WORKFLOW:
  1. Create spec: generator/specs/entities/my-entity.json
  2. Generate: npx tsx generator/EntityGenerator.ts specs/entities/my-entity.json
  3. Register in EntityRegistry.ts
  4. Schema auto-creates on first data operation
`;

    return {
      full,
      short: 'Usage: npx tsx generator/EntityGenerator.ts <spec.json>\nRun --help for full docs.',
      topics: {
        spec: full.split('FIELD TYPES')[0],
        types: full.split('FIELD TYPES')[1]?.split('VALIDATION')[0] || '',
        validation: full.split('VALIDATION RULES')[1]?.split('GENERATED')[0] || '',
      },
      availableTopics: ['spec', 'types', 'validation'],
    };
  }

  templateSpec(variant: string = 'standard'): EntitySpec {
    if (variant === 'minimal') {
      return {
        name: 'Example',
        collectionName: 'examples',
        description: 'Minimal entity example',
        fields: {
          label: { type: 'string', description: 'Display label' },
        },
      };
    }

    return {
      name: 'Example',
      collectionName: 'examples',
      description: 'Example entity with common field types',
      fields: {
        label: {
          type: 'string',
          description: 'Display label',
          validation: { minLength: 1, maxLength: 200 },
        },
        count: {
          type: 'number',
          description: 'Counter value',
          default: 0,
          validation: { min: 0 },
        },
        active: {
          type: 'boolean',
          description: 'Whether this entity is active',
          default: true,
        },
        tags: {
          type: 'array',
          description: 'Associated tags',
          arrayItemType: 'string',
          optional: true,
        },
        metadata: {
          type: 'JSON',
          description: 'Arbitrary metadata',
          optional: true,
        },
      },
      indexes: [
        { fields: ['label'], unique: true },
      ],
    };
  }

  templateVariants(): string[] {
    return ['minimal', 'standard'];
  }

  // ── Private Helpers ─────────────────────────────────────────────

  private discoverEntities(): Array<{ name: string; filePath: string }> {
    const results: Array<{ name: string; filePath: string }> = [];
    if (!fs.existsSync(this.entitiesDir)) return results;

    const files = fs.readdirSync(this.entitiesDir).filter(f =>
      f.endsWith('Entity.ts') && !f.startsWith('Base')
    );

    for (const file of files) {
      const name = file.replace('.ts', '');
      results.push({
        name,
        filePath: path.join(this.entitiesDir, file),
      });
    }

    return results;
  }

  private loadSpecMap(): Record<string, string> {
    const map: Record<string, string> = {};
    if (!fs.existsSync(this.specsDir)) return map;

    const files = fs.readdirSync(this.specsDir).filter(f => f.endsWith('.json'));
    for (const file of files) {
      const fullPath = path.join(this.specsDir, file);
      try {
        const spec: EntitySpec = JSON.parse(fs.readFileSync(fullPath, 'utf-8'));
        if (spec.name) {
          map[spec.name] = fullPath;
        }
      } catch {
        // Skip malformed specs
      }
    }

    return map;
  }

  private decoratorToFieldType(decorator: string): EntityField['type'] {
    switch (decorator) {
      case 'TextField': return 'string';
      case 'NumberField': return 'number';
      case 'BooleanField': return 'boolean';
      case 'DateField': return 'Date';
      case 'JsonField': return 'JSON';
      default: return 'string';
    }
  }
}
