#!/usr/bin/env npx tsx
/**
 * Guard against hand-built command directories.
 *
 * New command modules under src/commands must be backed by a committed
 * generator spec. The repo still has legacy commands without specs, so this
 * check is intentionally diff-scoped: it blocks new drift without making old
 * debt block every build.
 */

import * as fs from 'fs';
import * as path from 'path';
import { execFileSync } from 'child_process';

const DEFAULT_SRC_ROOT = path.resolve(__dirname, '..');
const COMMANDS_PREFIX = 'src/commands/';

interface GitFailure extends Error {
  status?: number;
  stderr?: Buffer | string;
}

export interface CommandSpecCoverageIssue {
  commandName: string;
  files: string[];
}

export interface CommandSpecCoverageResult {
  checkedCommands: number;
  missingSpecs: CommandSpecCoverageIssue[];
}

export interface CommandSpecCoverageOptions {
  srcRoot?: string;
  repoRoot?: string;
  baseRef?: string;
  stderr?: Pick<typeof process.stderr, 'write'>;
}

export function validateCommandSpecCoverage(options: CommandSpecCoverageOptions = {}): CommandSpecCoverageResult {
  const srcRoot = path.resolve(options.srcRoot ?? DEFAULT_SRC_ROOT);
  const repoRoot = path.resolve(options.repoRoot ?? path.join(srcRoot, '..'));
  const stderr = options.stderr ?? process.stderr;

  if (!isGitCheckout(repoRoot, stderr)) {
    return { checkedCommands: 0, missingSpecs: [] };
  }

  const specNames = loadSpecNames(path.join(srcRoot, 'generator', 'specs'));
  const addedPaths = addedCommandPaths(repoRoot, options.baseRef, stderr);
  const newCommands = new Map<string, string[]>();

  for (const filePath of addedPaths) {
    const commandName = commandNameFromPath(filePath);
    if (!commandName) continue;

    const current = newCommands.get(commandName) ?? [];
    current.push(filePath);
    newCommands.set(commandName, current);
  }

  const missingSpecs = Array.from(newCommands.entries())
    .filter(([commandName]) => !specNames.has(commandName))
    .map(([commandName, files]) => ({ commandName, files }))
    .sort((left, right) => left.commandName.localeCompare(right.commandName));

  return { checkedCommands: newCommands.size, missingSpecs };
}

function runGit(repoRoot: string, args: string[]): string {
  return execFileSync('git', args, {
    cwd: repoRoot,
    encoding: 'utf-8',
    stdio: ['ignore', 'pipe', 'pipe']
  }).trim();
}

function tryGit(repoRoot: string, args: string[], stderr: Pick<typeof process.stderr, 'write'>, quiet = false): string {
  try {
    return runGit(repoRoot, args);
  } catch (error) {
    if (!quiet) {
      const failure = error as GitFailure;
      const detail = Buffer.isBuffer(failure.stderr)
        ? failure.stderr.toString('utf-8').trim()
        : String(failure.stderr ?? '').trim();
      stderr.write(`Command spec coverage: git ${args.join(' ')} failed${detail ? `: ${detail}` : ''}\n`);
    }
    return '';
  }
}

function isGitCheckout(repoRoot: string, stderr: Pick<typeof process.stderr, 'write'>): boolean {
  return tryGit(repoRoot, ['rev-parse', '--show-toplevel'], stderr, true).length > 0;
}

function mergeBase(repoRoot: string, explicitBaseRef: string | undefined, stderr: Pick<typeof process.stderr, 'write'>): string {
  if (explicitBaseRef) {
    const explicitBase = tryGit(repoRoot, ['merge-base', explicitBaseRef, 'HEAD'], stderr);
    if (explicitBase) return explicitBase;
  }

  for (const ref of ['origin/canary', 'origin/main', 'canary', 'main']) {
    const base = tryGit(repoRoot, ['merge-base', ref, 'HEAD'], stderr, true);
    if (base) return base;
  }

  return '';
}

function splitLines(output: string): string[] {
  return output
    .split('\n')
    .map(line => line.trim())
    .filter(Boolean);
}

function addedCommandPaths(repoRoot: string, baseRef: string | undefined, stderr: Pick<typeof process.stderr, 'write'>): string[] {
  const paths = new Set<string>();
  const base = mergeBase(repoRoot, baseRef ?? process.env.COMMAND_SPEC_BASE_REF, stderr);

  if (base) {
    for (const filePath of splitLines(tryGit(repoRoot, ['diff', '--name-only', '--diff-filter=A', `${base}..HEAD`, '--', 'src/commands'], stderr))) {
      paths.add(filePath);
    }
  }

  for (const filePath of splitLines(tryGit(repoRoot, ['diff', '--name-only', '--diff-filter=A', 'HEAD', '--', 'src/commands'], stderr))) {
    paths.add(filePath);
  }

  for (const filePath of splitLines(tryGit(repoRoot, ['diff', '--cached', '--name-only', '--diff-filter=A', '--', 'src/commands'], stderr))) {
    paths.add(filePath);
  }

  for (const filePath of splitLines(tryGit(repoRoot, ['ls-files', '--others', '--exclude-standard', '--', 'src/commands'], stderr))) {
    paths.add(filePath);
  }

  return Array.from(paths).filter(filePath => filePath.startsWith(COMMANDS_PREFIX));
}

function loadSpecNames(specsDir: string): Set<string> {
  const specNames = new Set<string>();
  if (!fs.existsSync(specsDir)) return specNames;

  for (const fileName of fs.readdirSync(specsDir)) {
    if (!fileName.endsWith('.json')) continue;

    const specPath = path.join(specsDir, fileName);
    const raw = fs.readFileSync(specPath, 'utf-8');
    const parsed = JSON.parse(raw) as { name?: unknown };
    if (typeof parsed.name === 'string' && parsed.name.length > 0) {
      specNames.add(parsed.name);
    }
  }

  return specNames;
}

function commandNameFromPath(repoRelativePath: string): string | null {
  const commandRelative = repoRelativePath.slice(COMMANDS_PREFIX.length);
  const parts = commandRelative.split('/').filter(Boolean);
  if (parts.length === 0) return null;

  const moduleMarkerIndex = parts.findIndex(part =>
    part === 'shared' ||
    part === 'server' ||
    part === 'browser' ||
    part === 'test'
  );

  if (moduleMarkerIndex > 0) {
    return parts.slice(0, moduleMarkerIndex).join('/');
  }

  const leaf = parts[parts.length - 1];
  if (['README.md', 'package.json', '.npmignore'].includes(leaf) && parts.length > 1) {
    return parts.slice(0, -1).join('/');
  }

  return null;
}

function printMissingSpecs(missingSpecs: CommandSpecCoverageIssue[]): void {
  console.error('Command spec coverage: FAILED');
  console.error('New command modules must be generated from tools/generator/specs/*.json.');
  console.error('Do not create src/commands/** folders by hand.');
  console.error('');

  for (const issue of missingSpecs) {
    console.error(`- ${issue.commandName}`);
    for (const filePath of issue.files.slice(0, 5)) {
      console.error(`    ${filePath}`);
    }
    if (issue.files.length > 5) {
      console.error(`    ... ${issue.files.length - 5} more`);
    }
    console.error(`  Fix: add tools/generator/specs/${issue.commandName.replace(/\//g, '-')}.json and run:`);
    console.error(`       npx tsx generator/cli.ts command tools/generator/specs/${issue.commandName.replace(/\//g, '-')}.json --force`);
  }
}

export function main(): void {
  const result = validateCommandSpecCoverage();

  if (result.missingSpecs.length === 0) {
    console.log(`Command spec coverage: ok (${result.checkedCommands} new command module(s) checked)`);
    return;
  }

  printMissingSpecs(result.missingSpecs);
  process.exit(1);
}

if (path.resolve(process.argv[1] ?? '') === path.resolve(__filename)) {
  main();
}
