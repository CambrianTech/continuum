import type { DocMetadata } from '../list/shared/DocsListTypes';
import * as fs from 'fs/promises';
import * as path from 'path';

export class DocFileRegistry {
  private cache: DocMetadata[] | null = null;

  async discover(): Promise<DocMetadata[]> {
    if (this.cache) return this.cache;

    const projectRoot = process.cwd();
    const docs: DocMetadata[] = [];

    async function scan(dir: string): Promise<void> {
      try {
        const entries = await fs.readdir(dir, { withFileTypes: true });
        for (const entry of entries) {
          const fullPath = path.join(dir, entry.name);

          if (entry.isDirectory() && !entry.name.startsWith('.') && entry.name !== 'node_modules') {
            await scan(fullPath);
          } else if (entry.isFile() && entry.name.endsWith('.md')) {
            const stats = await fs.stat(fullPath);
            const content = await fs.readFile(fullPath, 'utf-8');
            const lines = content.split('\n');

            const sections = lines
              .filter(l => l.match(/^#{1,2}\s+/))
              .map(l => l.replace(/^#{1,2}\s+/, '').trim())
              .slice(0, 10);

            docs.push({
              name: pathToDocName(fullPath),
              filePath: fullPath,
              directory: path.dirname(fullPath).replace(projectRoot + '/', ''),
              sizeMB: stats.size / (1024 * 1024),
              lineCount: lines.length,
              lastModified: stats.mtime.toISOString(),
              sections
            });
          }
        }
      } catch (err) {
        // Skip inaccessible directories
      }
    }

    await scan(projectRoot);
    this.cache = docs.sort((a, b) => a.name.localeCompare(b.name));
    return this.cache;
  }
}

function pathToDocName(filePath: string): string {
  const relativePath = filePath.replace(/^.*\/(src\/debug\/jtag|continuum)\//, '');
  const withoutExt = relativePath.replace(/\.md$/, '');
  if (withoutExt.startsWith('docs/')) {
    return withoutExt.replace('docs/', '');
  }
  return withoutExt;
}
