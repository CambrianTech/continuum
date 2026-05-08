#!/usr/bin/env npx tsx

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { execFileSync } from 'child_process';
import { validateCommandSpecCoverage } from './validate-command-spec-coverage';

function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(`Assertion failed: ${message}`);
  }
  console.log(`ok - ${message}`);
}

function git(repoRoot: string, args: string[]): void {
  execFileSync('git', args, { cwd: repoRoot, stdio: 'ignore' });
}

function writeFile(filePath: string, content: string): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, content, 'utf-8');
}

function createRepo(): { repoRoot: string; srcRoot: string } {
  const repoRoot = fs.mkdtempSync(path.join(os.tmpdir(), 'continuum-command-spec-'));
  const srcRoot = path.join(repoRoot, 'src');
  fs.mkdirSync(path.join(srcRoot, 'commands'), { recursive: true });
  fs.mkdirSync(path.join(srcRoot, 'generator', 'specs'), { recursive: true });
  git(repoRoot, ['init']);
  git(repoRoot, ['config', 'user.email', 'test@example.invalid']);
  git(repoRoot, ['config', 'user.name', 'Command Spec Guard Test']);
  writeFile(path.join(srcRoot, 'README.md'), 'baseline\n');
  git(repoRoot, ['add', '.']);
  git(repoRoot, ['commit', '-m', 'baseline']);
  git(repoRoot, ['branch', 'canary']);
  return { repoRoot, srcRoot };
}

function runGuard(repoRoot: string, srcRoot: string): ReturnType<typeof validateCommandSpecCoverage> {
  return validateCommandSpecCoverage({
    repoRoot,
    srcRoot,
    baseRef: 'canary',
    stderr: { write: () => true },
  });
}

function testNewCommandWithoutSpecFails(): void {
  const { repoRoot, srcRoot } = createRepo();
  writeFile(path.join(srcRoot, 'commands', 'manual', 'server', 'ManualServerCommand.ts'), 'export {}\n');

  const result = runGuard(repoRoot, srcRoot);

  assert(result.missingSpecs.length === 1, 'new command without spec is reported');
  assert(result.missingSpecs[0].commandName === 'manual', 'missing command name is derived from server path');
}

function testNewCommandWithSpecPasses(): void {
  const { repoRoot, srcRoot } = createRepo();
  writeFile(path.join(srcRoot, 'commands', 'manual', 'server', 'ManualServerCommand.ts'), 'export {}\n');
  writeFile(path.join(srcRoot, 'generator', 'specs', 'manual.json'), JSON.stringify({ name: 'manual' }));

  const result = runGuard(repoRoot, srcRoot);

  assert(result.checkedCommands === 1, 'new command with spec is checked');
  assert(result.missingSpecs.length === 0, 'new command with matching spec passes');
}

function testRenameRequiresSpecForNewName(): void {
  const { repoRoot, srcRoot } = createRepo();
  writeFile(path.join(srcRoot, 'commands', 'old', 'server', 'OldServerCommand.ts'), 'export {}\n');
  writeFile(path.join(srcRoot, 'generator', 'specs', 'old.json'), JSON.stringify({ name: 'old' }));
  git(repoRoot, ['add', '.']);
  git(repoRoot, ['commit', '-m', 'old command']);
  git(repoRoot, ['branch', '-f', 'canary', 'HEAD']);

  fs.renameSync(path.join(srcRoot, 'commands', 'old'), path.join(srcRoot, 'commands', 'renamed'));

  const result = runGuard(repoRoot, srcRoot);

  assert(result.missingSpecs.length === 1, 'renamed command requires a spec for the new name');
  assert(result.missingSpecs[0].commandName === 'renamed', 'renamed command name is reported');
}

function testEditedExistingCommandPasses(): void {
  const { repoRoot, srcRoot } = createRepo();
  writeFile(path.join(srcRoot, 'commands', 'existing', 'server', 'ExistingServerCommand.ts'), 'export const value = 1;\n');
  git(repoRoot, ['add', '.']);
  git(repoRoot, ['commit', '-m', 'existing command']);
  git(repoRoot, ['branch', '-f', 'canary', 'HEAD']);

  writeFile(path.join(srcRoot, 'commands', 'existing', 'server', 'ExistingServerCommand.ts'), 'export const value = 2;\n');

  const result = runGuard(repoRoot, srcRoot);

  assert(result.checkedCommands === 0, 'edited existing command is not treated as a new command');
  assert(result.missingSpecs.length === 0, 'edited existing command passes without new spec requirement');
}

testNewCommandWithoutSpecFails();
testNewCommandWithSpecPasses();
testRenameRequiresSpecForNewName();
testEditedExistingCommandPasses();
console.log('Command spec coverage guard checks passed');
