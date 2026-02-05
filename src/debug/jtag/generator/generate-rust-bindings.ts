#!/usr/bin/env tsx
/**
 * Rust → TypeScript Binding Generator
 *
 * Runs ts-rs export tests for all Rust packages that define TypeScript types,
 * then generates barrel index.ts files for each output directory.
 *
 * Output: shared/generated/ (code/, persona/, rag/, ipc/, data-daemon/, etc.)
 *
 * Run manually: npx tsx generator/generate-rust-bindings.ts
 * Runs automatically as part of prebuild (after worker:build compiles Rust).
 */

import { execSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';

const ROOT = process.cwd();
const WORKERS_DIR = path.join(ROOT, 'workers');
const GENERATED_DIR = path.join(ROOT, 'shared', 'generated');

/**
 * Rust packages that export TypeScript types via ts-rs.
 * Each entry maps a cargo package name to its generated output subdirectories.
 */
const TS_RS_PACKAGES = [
  {
    package: 'continuum-core',
    description: 'Core IPC types (code, persona, rag, ipc, memory, voice)',
    // continuum-core exports to multiple subdirs: code/, persona/, rag/, ipc/
  },
  {
    package: 'data-daemon-worker',
    description: 'Data daemon storage adapter wire types',
    // Exports to: data-daemon/
  },
];

/**
 * Run cargo test to trigger ts-rs export for a package.
 * ts-rs v9 auto-generates export_bindings_* tests for each #[ts(export)] struct.
 */
function generateBindings(pkg: string, description: string): boolean {
  console.log(`  🦀 ${pkg}: ${description}`);
  try {
    execSync(
      `cargo test --package ${pkg} export_bindings --quiet`,
      {
        cwd: WORKERS_DIR,
        stdio: ['pipe', 'pipe', 'pipe'],
        timeout: 120_000,
      }
    );
    return true;
  } catch (error: any) {
    // Check if it's just "no tests matched" (not an error)
    const stderr = error.stderr?.toString() || '';
    if (stderr.includes('0 passed') || stderr.includes('running 0 tests')) {
      console.log(`     ⚠️  No export_bindings tests found — running all tests`);
      try {
        execSync(`cargo test --package ${pkg} --quiet`, {
          cwd: WORKERS_DIR,
          stdio: ['pipe', 'pipe', 'pipe'],
          timeout: 120_000,
        });
        return true;
      } catch (innerError: any) {
        console.error(`     ❌ Failed: ${innerError.stderr?.toString().slice(0, 200)}`);
        return false;
      }
    }
    console.error(`     ❌ Failed: ${stderr.slice(0, 200)}`);
    return false;
  }
}

/**
 * Scan a directory for generated .ts files and create a barrel index.ts
 */
function generateBarrelExport(dir: string): void {
  if (!fs.existsSync(dir)) return;

  const files = fs.readdirSync(dir)
    .filter(f => f.endsWith('.ts') && f !== 'index.ts')
    .sort();

  if (files.length === 0) return;

  const dirName = path.basename(dir);
  const exports = files
    .map(f => {
      const typeName = f.replace('.ts', '');
      return `export type { ${typeName} } from './${typeName}';`;
    })
    .join('\n');

  const content = `// Auto-generated barrel export — do not edit manually
// Source: generator/generate-rust-bindings.ts
// Re-generate: npx tsx generator/generate-rust-bindings.ts

${exports}
`;

  fs.writeFileSync(path.join(dir, 'index.ts'), content);
  console.log(`  📦 ${dirName}/index.ts (${files.length} types)`);
}

/**
 * Parse the exported type name(s) from a ts-rs generated .ts file.
 * ts-rs files use: export type TypeName = { ... };
 * The type name may not match the filename (e.g., RagTypes.ts exports MessageRole).
 */
function parseExportedTypes(filePath: string): string[] {
  const content = fs.readFileSync(filePath, 'utf-8');
  const matches = content.matchAll(/export type\s+(\w+)/g);
  return Array.from(matches, m => m[1]);
}

/**
 * Generate the master barrel at shared/generated/index.ts
 */
function generateMasterBarrel(): void {
  const subdirs = fs.readdirSync(GENERATED_DIR, { withFileTypes: true })
    .filter(d => d.isDirectory())
    .map(d => d.name)
    .sort();

  // Collect top-level .ts files (e.g., CallMessage.ts)
  const topLevelFiles = fs.readdirSync(GENERATED_DIR)
    .filter(f => f.endsWith('.ts') && f !== 'index.ts')
    .sort();

  // Collect all type names exported by subdirectories (to detect duplicates)
  const subdirTypes = new Set<string>();
  for (const dir of subdirs) {
    const dirPath = path.join(GENERATED_DIR, dir);
    const files = fs.readdirSync(dirPath).filter(f => f.endsWith('.ts') && f !== 'index.ts');
    for (const f of files) {
      const types = parseExportedTypes(path.join(dirPath, f));
      types.forEach(t => subdirTypes.add(t));
    }
  }

  const lines: string[] = [
    '// Auto-generated master barrel — do not edit manually',
    '// Source: generator/generate-rust-bindings.ts',
    '// Re-generate: npx tsx generator/generate-rust-bindings.ts',
    '',
  ];

  // Re-export subdirectories
  for (const dir of subdirs) {
    const indexPath = path.join(GENERATED_DIR, dir, 'index.ts');
    if (fs.existsSync(indexPath)) {
      lines.push(`export * from './${dir}';`);
    }
  }

  // Re-export top-level files, skipping types already exported by subdirectories
  for (const file of topLevelFiles) {
    const filePath = path.join(GENERATED_DIR, file);
    const types = parseExportedTypes(filePath);
    const moduleName = file.replace('.ts', '');

    for (const typeName of types) {
      if (subdirTypes.has(typeName)) {
        console.log(`  ⚠️  Skipping ${file} → ${typeName} (already exported by subdirectory)`);
        continue;
      }
      lines.push(`export type { ${typeName} } from './${moduleName}';`);
    }
  }

  lines.push('');

  fs.writeFileSync(path.join(GENERATED_DIR, 'index.ts'), lines.join('\n'));
  console.log(`  📦 index.ts (master barrel)`);
}

async function main() {
  console.log('🔧 Generating Rust → TypeScript bindings via ts-rs...\n');

  // Ensure output directory exists
  fs.mkdirSync(GENERATED_DIR, { recursive: true });

  // Step 1: Run cargo test for each package to generate .ts files
  let allSuccess = true;
  for (const pkg of TS_RS_PACKAGES) {
    const ok = generateBindings(pkg.package, pkg.description);
    if (!ok) allSuccess = false;
  }

  if (!allSuccess) {
    console.error('\n❌ Some bindings failed to generate');
    process.exit(1);
  }

  console.log('');

  // Step 2: Generate barrel index.ts for each subdirectory
  console.log('📦 Generating barrel exports...');
  const subdirs = fs.readdirSync(GENERATED_DIR, { withFileTypes: true })
    .filter(d => d.isDirectory())
    .map(d => d.name);

  for (const dir of subdirs) {
    generateBarrelExport(path.join(GENERATED_DIR, dir));
  }

  // Step 3: Generate master barrel
  generateMasterBarrel();

  // Count total types
  let totalTypes = 0;
  for (const dir of subdirs) {
    const dirPath = path.join(GENERATED_DIR, dir);
    totalTypes += fs.readdirSync(dirPath).filter(f => f.endsWith('.ts') && f !== 'index.ts').length;
  }

  console.log(`\n✅ Generated ${totalTypes} TypeScript types from Rust via ts-rs`);
}

main().catch(error => {
  console.error('❌ Rust binding generation failed:', error);
  process.exit(1);
});
