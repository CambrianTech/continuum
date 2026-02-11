/**
 * AI Challenge: Fix this file!
 *
 * There are 3 bugs in this code:
 * 1. A missing import
 * 2. A type error
 * 3. A logic error
 *
 * Use code/read to examine, code/write to fix, then BuildSentinel to verify.
 */

// BUG 1: Missing path import (fs is imported but path is not)
import * as fs from 'fs';
import * as path from 'path';

interface FileInfo {
  name: string;
  size: number;
  isDirectory: boolean;
}

export function listFiles(directory: string): FileInfo[] {
  const entries = fs.readdirSync(directory, { withFileTypes: true });

  return entries.map((entry) => {
    // BUG 2: Type error - fullPath is declared but wrong type
    const fullPath: number = path.join(directory, entry.name);

    const stats = fs.statSync(fullPath);

    return {
      name: entry.name,
      size: stats.size,
      // BUG 3: Logic error - should check entry.isDirectory() not stats.isFile()
      isDirectory: stats.isFile(),
    };
  });
}

export function getFileContent(filePath: string): string {
  return fs.readFileSync(filePath, 'utf-8');
}
