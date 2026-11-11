// Publication and adoption use the lifecycle crate's single build-input definition.
import { readFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
const execute = promisify(execFile);
export async function resolveBuildKey(tip, cwd) {
  if (!/^[0-9a-f]{40}$/.test(tip)) throw new Error('build-key tip must be a full SHA');
  const text = await readFile(new URL('../../core/continuum-cli-lifecycle/build-inputs.txt', import.meta.url), 'utf8');
  const paths = text.trim().split(/\r?\n/);
  if (!paths.length || paths.some(path => !path || path.startsWith('-') || path.includes('..'))) {
    throw new Error('invalid lifecycle build inputs');
  }
  const { stdout } = await execute('git', ['log', '-1', '--format=%H', tip, '--', ...paths], { cwd });
  const key = stdout.trim();
  if (!/^[0-9a-f]{40}$/.test(key)) throw new Error('build key could not be resolved');
  return key;
}
