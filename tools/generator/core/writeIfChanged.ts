/**
 * Write-if-changed utility for generators
 *
 * Compares new content against existing file and only writes when different.
 * Prevents build artifacts from dirtying git on every npm start.
 */

import { readFileSync, writeFileSync, existsSync } from 'fs';

export function writeIfChanged(filePath: string, content: string): boolean {
  if (existsSync(filePath)) {
    const existing = readFileSync(filePath, 'utf-8');
    if (existing === content) {
      return false;
    }
  }
  writeFileSync(filePath, content, 'utf-8');
  return true;
}
