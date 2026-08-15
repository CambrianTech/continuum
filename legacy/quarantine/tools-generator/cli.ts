#!/usr/bin/env npx tsx
/**
 * CLI entry point for generators
 *
 * Usage:
 *   npx tsx generator/cli.ts command generator/specs/my-command.json
 *   npx tsx generator/cli.ts command generator/specs/my-command.json --force
 *   npx tsx generator/cli.ts audit
 */

import { createGeneratorRegistry } from './GeneratorSDKFactory';
import * as path from 'path';

const args = process.argv.slice(2);

if (args.length === 0) {
  console.error('Usage: npx tsx generator/cli.ts <type> <spec.json> [--force]');
  console.error('       npx tsx generator/cli.ts audit');
  console.error('Types: command, entity, daemon, widget');
  process.exit(1);
}

const rootPath = path.resolve(__dirname, '..');
const registry = createGeneratorRegistry(rootPath);

const mode = args[0];

if (mode === 'audit') {
  const audits = registry.auditAll();
  registry.printAuditReport(audits);
} else {
  // Generate mode: <type> <spec-file> [--force]
  const specFile = args[1];
  if (!specFile) {
    console.error(`Missing spec file. Usage: npx tsx generator/cli.ts ${mode} <spec.json>`);
    process.exit(1);
  }

  const force = args.includes('--force');
  const generatorType = registry.get(mode);
  if (!generatorType) {
    console.error(`Unknown generator type: ${mode}. Available: ${registry.types().join(', ')}`);
    process.exit(1);
  }

  const resolvedSpec = path.resolve(specFile);
  generatorType.generateFromFile(resolvedSpec, undefined, { force });
}
