#!/usr/bin/env tsx
/**
 * Rust → TypeScript Binding Generator
 *
 * Runs ts-rs export tests for all Rust packages that define TypeScript types,
 * then generates barrel index.ts files for each output directory.
 *
 * Output: shared/generated/ (code/, persona/, rag/, ipc/, data/, etc.)
 *
 * Run manually: npx tsx generator/generate-rust-bindings.ts
 * Runs automatically as part of prebuild via smart-build.ts.
 *
 * IMPORTANT: This runs `cargo test --release` which shares the target directory with
 * `cargo build --release`. parallel-start.sh ensures cargo build finishes BEFORE
 * this script runs (via prebuild), so the compilation cache is warm and binding
 * generation takes ~4s instead of a full rebuild (~2.5 min).
 */

import { spawnSync } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { Preflight } from '../scripts/shared/Preflight';

const ROOT = process.cwd();
const WORKERS_DIR = path.join(ROOT, 'workers');
const GENERATED_DIR = path.join(ROOT, 'shared', 'generated');

/**
 * Detect GPU features for cargo commands — mirrors scripts/shared/cargo-features.sh.
 * Returns args like ['--features', 'metal,accelerate'] or ['--features', 'cuda'] or [].
 */
function detectGpuFeatures(): string[] {
  const platform = process.platform;
  if (platform === 'darwin') {
    return ['--features', 'metal,accelerate'];
  }
  if (platform === 'linux') {
    // Check for NVIDIA GPU (standard path + WSL path)
    const hasCuda = fs.existsSync('/usr/lib/wsl/lib/nvidia-smi') ||
      spawnSync('which', ['nvidia-smi'], { stdio: 'pipe' }).status === 0;
    if (hasCuda) {
      return ['--features', 'cuda'];
    }
  }
  return []; // CPU-only
}

/**
 * Rust packages that export TypeScript types via ts-rs.
 * Each entry maps a cargo package name to its generated output subdirectories.
 */
const TS_RS_PACKAGES = [
  {
    package: 'continuum-core',
    description: 'Core IPC types (code, persona, rag, ipc, memory, voice, data)',
    // continuum-core exports to multiple subdirs: code/, persona/, rag/, ipc/, data/
    // NOTE: data-daemon-worker removed - DataModule now in continuum-core
  },
];

/**
 * Run cargo test to trigger ts-rs export for a package.
 * ts-rs v9 auto-generates export_bindings_* tests for each #[ts(export)] struct.
 *
 * Uses spawnSync (not execSync) for proper exit/signal handling.
 * --lib flag restricts to library tests only, skipping integration test binaries
 * that link webrtc-sys and hang during cleanup in release mode.
 */
function generateBindings(pkg: string, description: string): boolean {
  console.log(`  🦀 ${pkg}: ${description}`);

  // --release: livekit's webrtc-sys native library only builds in release mode
  // --lib: only lib tests (export_bindings live there), avoids webrtc-sys cleanup hangs
  // GPU features: must match the build features (metal on macOS, cuda on Linux)
  const gpuFeatures = detectGpuFeatures();
  const args = ['test', '--package', pkg, '--lib', 'export_bindings', '--release', ...gpuFeatures];
  // Timeout default 900s (was 300s, raised in #980 Bug 2). On a cold M1 the
  // partially-cached --no-run compile measured 192s; cold-cold scenarios on
  // slower hardware (CI runners, older Macs) routinely blow past 300s,
  // causing Phase 2b to fail with a cryptic "Timed out after 300s" → "npm
  // run prebuild failed" cascade. Env-overridable via
  // CONTINUUM_TS_RS_TIMEOUT_MS for users on faster hardware who want a
  // tighter feedback loop, OR for CI lanes that genuinely need to bail
  // sooner on a wedged build.
  const timeoutMs = parseInt(process.env.CONTINUUM_TS_RS_TIMEOUT_MS ?? '', 10) || 900_000;
  const result = spawnSync(
    'cargo',
    args,
    {
      cwd: WORKERS_DIR,
      stdio: ['pipe', 'pipe', 'pipe'],
      timeout: timeoutMs,
    }
  );

  const stdout = result.stdout?.toString() || '';
  const stderr = result.stderr?.toString() || '';

  // Success: exit 0
  if (result.status === 0) {
    return true;
  }

  // SIGSEGV during cleanup (webrtc-sys) — bindings already generated
  if (result.signal === 'SIGSEGV' && !stderr.includes('could not compile') && !stderr.includes('error[')) {
    console.log(`     ⚠️  WebRTC native cleanup crash (SIGSEGV) — bindings generated OK`);
    return true;
  }

  // Tests passed but process crashed during cleanup
  if (stdout.includes('test result: ok') || (stdout + stderr).includes('passed')) {
    console.log(`     ⚠️  Tests passed but process exited abnormally (signal: ${result.signal}) — bindings OK`);
    return true;
  }

  // ts-rs warnings (harmless)
  if (stderr.includes('ts-rs failed to parse this attribute') &&
      !stderr.includes('error[') && !stderr.includes('error:') &&
      !stderr.includes('could not compile')) {
    console.log(`     ⚠️  ts-rs warnings (ignored)`);
    return true;
  }

  // Timeout
  if (result.signal === 'SIGTERM') {
    console.error(`     ❌ Timed out after 300s`);
    return false;
  }

  // Build tool issues — actionable messages
  const buildFailure = Preflight.detectCargoBuildFailure(stderr);
  if (buildFailure) {
    console.error(`     ${buildFailure.message}`);
    return false;
  }

  // Real failure
  const errorDetail = stderr.slice(0, 500) || stdout.slice(0, 500) || `exit=${result.status} signal=${result.signal}`;
  console.error(`     ❌ Failed: ${errorDetail}`);
  return false;
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
      const moduleName = f.replace('.ts', '');
      const exportedTypes = parseExportedTypes(path.join(dir, f));
      // Use the ACTUAL exported type names from the file, not the filename
      if (exportedTypes.length > 0) {
        return exportedTypes.map(t => `export type { ${t} } from './${moduleName}';`).join('\n');
      }
      // Fallback: assume filename matches type name (legacy behavior)
      return `export type { ${moduleName} } from './${moduleName}';`;
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
 * Handles duplicate type names across subdirectories by using explicit exports.
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

  // Map: typeName -> first subdir that exports it
  // This detects duplicates across subdirectories
  const typeToDir = new Map<string, string>();
  const duplicateTypes = new Set<string>();

  for (const dir of subdirs) {
    const dirPath = path.join(GENERATED_DIR, dir);
    const files = fs.readdirSync(dirPath).filter(f => f.endsWith('.ts') && f !== 'index.ts');
    for (const f of files) {
      const types = parseExportedTypes(path.join(dirPath, f));
      for (const t of types) {
        if (typeToDir.has(t)) {
          duplicateTypes.add(t);
          console.log(`  ⚠️  Duplicate type '${t}' in ${dir} (first seen in ${typeToDir.get(t)})`);
        } else {
          typeToDir.set(t, dir);
        }
      }
    }
  }

  const lines: string[] = [
    '// Auto-generated master barrel — do not edit manually',
    '// Source: generator/generate-rust-bindings.ts',
    '// Re-generate: npx tsx generator/generate-rust-bindings.ts',
    '',
  ];

  // For directories with NO duplicate types, use wildcard export
  // For directories WITH duplicate types, use explicit exports (excluding duplicates)
  for (const dir of subdirs) {
    const indexPath = path.join(GENERATED_DIR, dir, 'index.ts');
    if (!fs.existsSync(indexPath)) continue;

    const dirPath = path.join(GENERATED_DIR, dir);
    const files = fs.readdirSync(dirPath).filter(f => f.endsWith('.ts') && f !== 'index.ts');

    // Collect types in this directory
    const dirTypes: string[] = [];
    for (const f of files) {
      const types = parseExportedTypes(path.join(dirPath, f));
      dirTypes.push(...types);
    }

    // Check if this dir has any duplicates
    const hasDuplicates = dirTypes.some(t => duplicateTypes.has(t));

    if (!hasDuplicates) {
      // Safe to use wildcard
      lines.push(`export * from './${dir}';`);
    } else {
      // Use explicit exports, skipping types that are duplicated
      // Only export from the FIRST directory that had the type
      lines.push(`// ${dir}: explicit exports (has duplicate types)`);
      for (const typeName of dirTypes) {
        if (duplicateTypes.has(typeName)) {
          // Only export if this is the first directory
          if (typeToDir.get(typeName) === dir) {
            lines.push(`export type { ${typeName} } from './${dir}';`);
          }
          // else skip - it's exported from another dir
        } else {
          lines.push(`export type { ${typeName} } from './${dir}';`);
        }
      }
    }
  }

  // Re-export top-level files, skipping types already exported by subdirectories
  for (const file of topLevelFiles) {
    const filePath = path.join(GENERATED_DIR, file);
    const types = parseExportedTypes(filePath);
    const moduleName = file.replace('.ts', '');

    for (const typeName of types) {
      if (typeToDir.has(typeName)) {
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

/**
 * Check if Rust source files have changed since bindings were last generated.
 * Compares newest .rs mtime in workers/ against newest generated .ts mtime in shared/generated/.
 * Skips cargo test entirely when Rust hasn't changed — saves 12-300s per build.
 */
function bindingsUpToDate(): boolean {
  // Find newest generated .ts file
  let newestGenerated = 0;
  try {
    const subdirs = fs.readdirSync(GENERATED_DIR, { withFileTypes: true })
      .filter(d => d.isDirectory());
    for (const dir of subdirs) {
      const dirPath = path.join(GENERATED_DIR, dir.name);
      const files = fs.readdirSync(dirPath).filter(f => f.endsWith('.ts') && f !== 'index.ts');
      for (const f of files) {
        const mtime = fs.statSync(path.join(dirPath, f)).mtime.getTime();
        if (mtime > newestGenerated) newestGenerated = mtime;
      }
    }
  } catch {
    return false; // No generated files — must regenerate
  }

  if (newestGenerated === 0) return false;

  // Find newest .rs file in workers/
  let newestRust = 0;
  function scanDir(dir: string) {
    try {
      const entries = fs.readdirSync(dir, { withFileTypes: true });
      for (const entry of entries) {
        const fullPath = path.join(dir, entry.name);
        if (entry.isDirectory() && entry.name !== 'target' && entry.name !== '.git') {
          scanDir(fullPath);
        } else if (entry.isFile() && entry.name.endsWith('.rs')) {
          const mtime = fs.statSync(fullPath).mtime.getTime();
          if (mtime > newestRust) newestRust = mtime;
        }
      }
    } catch { /* skip inaccessible dirs */ }
  }
  scanDir(WORKERS_DIR);

  // Also check Cargo.toml files (dependency changes can affect generated types)
  try {
    const cargoToml = fs.statSync(path.join(WORKERS_DIR, 'Cargo.toml')).mtime.getTime();
    if (cargoToml > newestRust) newestRust = cargoToml;
    const coreCargo = fs.statSync(path.join(WORKERS_DIR, 'continuum-core', 'Cargo.toml')).mtime.getTime();
    if (coreCargo > newestRust) newestRust = coreCargo;
  } catch { /* missing Cargo.toml is fine */ }

  return newestRust <= newestGenerated;
}

async function main() {
  console.log('🔧 Generating Rust → TypeScript bindings via ts-rs...\n');

  // Ensure output directory exists
  fs.mkdirSync(GENERATED_DIR, { recursive: true });

  // ONLY macOS regenerates ts-rs bindings (they're generated there and
  // committed to git). Everyone else — Linux/WSL towers, the carl-install-smoke
  // CI container — uses the committed bindings as-is and NEVER runs cargo:
  //   * cargo test crashes on Linux (LiveKit protobuf descriptor conflict), and
  //   * a cold continuum-core + llama.cpp compile blows past the install/build window.
  // Pre-fix this skip was gated on `bindingsExist` checking GENERATED_DIR
  // (shared/generated) — but the committed bindings live in protocol/typescript,
  // so GENERATED_DIR read EMPTY on a fresh clone, the guard was false, and Linux
  // fell through to the broken cargo path → install.sh's `npm run build` died
  // ("Some bindings failed to generate") = the carl-install-smoke failure (C8).
  // Gate on the platform itself (not a dir that moved) so the skip is robust.
  const isMac = process.platform === 'darwin';
  const forceRegen = process.argv.includes('--force');
  if (!isMac && !forceRegen) {
    console.log('  ✅ Non-macOS: using committed bindings (cargo skipped — generated + committed on macOS)\n');
  } else if (!forceRegen && bindingsUpToDate()) {
    console.log('  ✅ Bindings up to date (no Rust changes since last generation)');
    console.log('     Use --force to regenerate anyway\n');
  } else {
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
