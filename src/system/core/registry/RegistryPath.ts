/**
 * Registry Path Resolution - Delegates to SystemPaths
 */

import fs from 'fs';
import { SystemPaths } from '../config/SystemPaths';

export function getRegistryDir(): string {
  return SystemPaths.registry.root;
}

export function getRegistryFile(): string {
  return SystemPaths.registry.processes;
}

export function ensureRegistryDir(): void {
  if (!fs.existsSync(SystemPaths.registry.root)) {
    fs.mkdirSync(SystemPaths.registry.root, { recursive: true });
  }
}
